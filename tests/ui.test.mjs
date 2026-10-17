import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
const html=readFileSync(new URL('../ui/index.html',import.meta.url),'utf8');
const js=readFileSync(new URL('../ui/app.js',import.meta.url),'utf8');
const rust=readFileSync(new URL('../src-tauri/src/lib.rs',import.meta.url),'utf8');
const conf=JSON.parse(readFileSync(new URL('../src-tauri/tauri.conf.json',import.meta.url),'utf8'));
test('four actions, no sleep or hibernate',()=>{
  assert.deepEqual([...html.matchAll(/data-action="([^"]+)"/g)].map(x=>x[1]),['shutdown','restart','lock','logoff']);
  assert.doesNotMatch(html,/data-action="(?:sleep|hibernate)"/);
});
test('four scheduling conditions',()=>{
  assert.deepEqual([...html.matchAll(/data-trigger="([^"]+)"/g)].map(x=>x[1]),['interval','clock','idle','cpu']);
});
test('native controls and test mode',()=>{
  for(const name of ['start_timer','cancel_timer','extend_timer','resolve_warning','get_snapshot','update_settings','minimize_main','close_main']){
    assert.match(rust,new RegExp(`fn ${name}\\(`));
  }
  assert.match(rust,/test_mode:true/);
  assert.match(rust,/Plan/);
  assert.match(rust,/read_ini/);
  assert.match(rust,/warning_deadline/);
});
test('security and portable policy',()=>{
  assert.equal(conf.app.withGlobalTauri,true);
  assert.equal(conf.app.windows[0].resizable,false);
  assert.equal(conf.app.windows[0].width,1200);
  assert.equal(conf.app.windows[0].height,680);
  assert.doesNotMatch(js,/eval\s*\(/);
});

test('clean header and local clock',()=>{
  assert.doesNotMatch(html,/УПРАВЛЕНИЕ ПИТАНИЕМ|Таймер компьютера|Запланируйте действие Windows/);
  assert.doesNotMatch(html,/class="intro"/);
  assert.match(html,/class="titlebar-clock"/);
  assert.match(html,/<time id="now"(?:\s+[^>]*)?>/);
});
test('GitHub source edition uses locally installed Inter and no distributed font binaries',()=>{
  const css=readFileSync(new URL('../ui/style.css',import.meta.url),'utf8');
  const trayCss=readFileSync(new URL('../ui/tray.css',import.meta.url),'utf8');
  assert.match(css,/font:15px\/1\.45 "Inter Embedded"/);
  for (const weight of ['Regular','Medium','SemiBold','Bold','ExtraBold']) {
    assert.ok(css.includes(`local("Inter ${weight}")`));
    assert.ok(!css.includes(`Inter-${weight}.woff2`));
  }
  assert.match(trayCss,/local\('Inter Regular'\)/);
  assert.doesNotMatch(trayCss,/\.woff2/);
});

test('comfortable 1200×680 layout retains all controls',()=>{
  const css=readFileSync(new URL('../ui/style.css',import.meta.url),'utf8');
  assert.equal(conf.app.windows[0].minHeight,680);
  assert.match(css,/min-height:680px/);
  assert.match(css,/\.footer-panel\{height:94px/);
  for(const id of ['action-grid','trigger-interval','trigger-clock','trigger-idle','trigger-cpu','warning-seconds','cpu-readout','idle-readout']){
    assert.match(html,new RegExp(`id="${id}"`));
  }
});
test('remaining time is supplied by Rust and rendered in footer',()=>{
  for(const id of ['remaining-time','remaining-label','remaining-value','footer-panel','progress-track']) {
    assert.match(html,new RegExp(`id="${id}"`));
  }
  assert.match(rust,/remaining_seconds:Option<u64>/);
  assert.match(rust,/saturating_duration_since\(now\)/);
  assert.match(js,/s\.remainingSeconds/);
  assert.match(js,/function fmtCountdown\(value\)/);
  assert.match(js,/aria-valuenow/);
});
test('two avoidable Rust compiler warnings removed',()=>{
  assert.doesNotMatch(rust,/\bstart:Instant/);
  assert.doesNotMatch(rust,/\bstart:now/);
  assert.doesNotMatch(rust,/let mut total_seconds=0\.0/);
  assert.match(rust,/let total_seconds;/);
});


test('larger UI font sizes and buttons preserve local Inter',()=>{
  const css=readFileSync(new URL('../ui/style.css',import.meta.url),'utf8');
  assert.match(css,/font:15px\/1\.45 "Inter Embedded"/);
  for(const selector of ['.panel-heading h2','.action-option','.trigger-option','.setting-content strong','.status-heading strong']){
    assert.match(css,new RegExp(selector.replaceAll('.','\\.')+'\\{[^}]*font-size:(?:12|13|13\\.5|14|16)px'));
  }
  assert.match(css,/\.stepper\{height:45px/);
  assert.match(css,/\.btn\{height:46px/);
  assert.equal(conf.app.windows[0].maximizable,false);
});


test('buttons behave like the earlier apps: hover, press, keyboard focus and reduced motion',()=>{
  const css=readFileSync(new URL('../ui/style.css',import.meta.url),'utf8');
  assert.match(css,/@media \(hover:hover\) and \(pointer:fine\)/);
  assert.match(css,/\.action-option:not\(:disabled\):active/);
  assert.match(css,/\.preset-row button:not\(:disabled\):active/);
  assert.match(css,/\.btn:not\(:disabled\):active/);
  assert.match(css,/\.window-button\{[^}]*transform:none!important/);
  assert.match(css,/button:focus-visible/);
  assert.match(css,/@media \(prefers-reduced-motion:reduce\)/);
});
test('modal Enter always operates focused button, and keyboard navigation stays in modal',()=>{
  assert.match(js,/warningReturnFocus/);
  assert.match(js,/warning-cancel'\)\.focus/);
  assert.match(js,/event\.key==='Tab'/);
  assert.match(js,/event\.key==='Enter'/);
  assert.doesNotMatch(js,/event\.key==='Enter'.*perform\('resolve_warning',\{proceed:true\}\)/);
  assert.match(html,/Enter — выбранная кнопка/);
});
test('preset pressed states, busy guard and titlebar match native conventions',()=>{
  assert.match(js,/el\.setAttribute\('aria-pressed',String\(selected\)\)/);
  assert.match(js,/Boolean\(active\)\|\|busy/);
  for(const id of ['minimize-btn','close-btn']){
    const el=html.match(new RegExp(`<button[^>]*id="${id}"[^>]*>`));
    assert.ok(el);
    assert.doesNotMatch(el[0],/data-tauri-drag-region/);
  }
  assert.doesNotMatch(html,/class="app-version"/);
  assert.match(html,/<div class="app-name" data-tauri-drag-region>PowerTimer<\/div>/);
});

test('frameless Windows titlebar removes DWM white shadow and allows drag',()=>{
  const cap=JSON.parse(readFileSync(new URL('../src-tauri/capabilities/default.json',import.meta.url),'utf8'));
  const css=readFileSync(new URL('../ui/style.css',import.meta.url),'utf8');
  assert.equal(conf.app.windows[0].shadow,false);
  assert.ok(cap.permissions.includes('core:window:allow-start-dragging'));
  assert.match(html,/header class="titlebar" data-tauri-drag-region/);
  assert.match(css,/\.shell\{border:0;box-shadow:none;outline:0/);
  assert.match(html,/id="close-btn"/);
  assert.match(html,/id="minimize-btn"/);
});
test('custom dark Inter tray popup replaces white native context menu',()=>{
  const trayHtml=readFileSync(new URL('../ui/tray.html',import.meta.url),'utf8');
  const trayCss=readFileSync(new URL('../ui/tray.css',import.meta.url),'utf8');
  const trayJs=readFileSync(new URL('../ui/tray.js',import.meta.url),'utf8');
  const tray=conf.app.windows.find(x=>x.label==='tray-menu');
  assert.ok(tray);
  assert.equal(tray.visible,false);
  assert.equal(tray.shadow,false);
  assert.equal(tray.skipTaskbar,true);
  assert.equal(tray.url,'tray.html');
  assert.match(trayCss,/InterMini/);
  for(const action of ['cancel','quit']){
    assert.match(trayHtml,new RegExp(`data-tray-action="${action}"`));
  }
  assert.doesNotMatch(trayHtml,/data-tray-action="(?:open|extend)"/);
  assert.doesNotMatch(trayJs,/tray-extend/);
  assert.match(trayJs,/tray_menu_action/);
  assert.match(rust,/MouseButton::Right/);
  assert.doesNotMatch(rust,/\.menu\(&menu\)/);
});
test('native window events do not hide main window when tray popup loses focus',()=>{
  assert.match(rust,/window\.label\(\)=="tray-menu"/);
  assert.match(rust,/WindowEvent::Focused\(false\)/);
  assert.match(rust,/toggle_tray_popup/);
  assert.match(rust,/\.set_position\(tauri::PhysicalPosition::new\(x,y\)\)/);
  assert.match(rust,/fn tray_menu_action\(/);
});


test('PowerTimer brand, bundled icon and renamed executable',()=>{
  const trayHtml=readFileSync(new URL('../ui/tray.html',import.meta.url),'utf8');
  const cargo=readFileSync(new URL('../src-tauri/Cargo.toml',import.meta.url),'utf8');
  const main=readFileSync(new URL('../src-tauri/src/main.rs',import.meta.url),'utf8');
  assert.equal(conf.productName,'PowerTimer');
  assert.equal(conf.version,'0.8.0');
  assert.equal(conf.app.windows[0].title,'PowerTimer');
  assert.match(cargo,/name = "powertimer"/);
  assert.match(cargo,/name = "powertimer_lib"/);
  assert.match(main,/powertimer_lib::run/);
  assert.doesNotMatch(html,/PowerTimer\s+<strong[^>]*>Mini|PowerTimer Mini/);
  assert.doesNotMatch(trayHtml,/PowerTimer Mini/);
  for(const relative of ['../src-tauri/icons/icon.png','../src-tauri/icons/icon.ico','../ui/app-icon.png']){
    assert.ok(readFileSync(new URL(relative,import.meta.url)).length>1000,relative);
  }
});

test('tray menu is limited to cancel and quit, icon click reopens window',()=>{
  const trayHtml=readFileSync(new URL('../ui/tray.html',import.meta.url),'utf8');
  const trayJs=readFileSync(new URL('../ui/tray.js',import.meta.url),'utf8');
  assert.deepEqual([...trayHtml.matchAll(/data-tray-action="([^"]+)"/g)].map(x=>x[1]),['cancel','quit']);
  assert.match(rust,/MouseButton::Left,button_state:MouseButtonState::Up/);
  assert.match(rust,/DoubleClick \{button:MouseButton::Left/);
  assert.match(rust,/show_window\(tray\.app_handle\(\)\)/);
  assert.doesNotMatch(rust,/"open"=>/);
  assert.doesNotMatch(rust,/"extend"=>/);
  assert.doesNotMatch(trayJs,/tray-extend/);
  assert.equal(conf.app.windows.find(w=>w.label==='tray-menu').height,242);
});

test('portable settings remain compatible with older versions',()=>{
  assert.match(rust,/appdata\.join\("PowerTimerMini"\)/);
  assert.match(rust,/join\("config.ini"\)/);
  assert.match(rust,/PowerTimerMini\.log/);
});

test('0.7.7 centred draggable header without icon and edition badge',()=>{
  const css=readFileSync(new URL('../ui/style.css',import.meta.url),'utf8');
  const header=html.match(/<header class="titlebar"[\s\S]*?<\/header>/)?.[0];
  assert.ok(header);
  assert.match(header,/data-tauri-drag-region/);
  assert.match(header,/class="app-name" data-tauri-drag-region>PowerTimer<\/div>/);
  assert.doesNotMatch(header,/app-mark|app-version|TAURI EDITION|app-icon\.png/);
  assert.match(css,/\.titlebar\{position:relative;padding-left:0\}/);
  assert.match(css,/\.titlebar \.app-name\{position:absolute;left:50%;top:50%;transform:translate\(-50%,-50%\)/);
  assert.match(css,/pointer-events:none/);
  for(const id of ['minimize-btn','close-btn']) assert.match(header,new RegExp(`id="${id}"`));
});
test('0.7.7 compact tray omits subtitle and bottom hint, keeps all actions',()=>{
  const trayHtml=readFileSync(new URL('../ui/tray.html',import.meta.url),'utf8');
  const trayJs=readFileSync(new URL('../ui/tray.js',import.meta.url),'utf8');
  assert.doesNotMatch(trayHtml,/БЫСТРОЕ УПРАВЛЕНИЕ|tray-hint/);
  assert.doesNotMatch(trayJs,/tray-hint/);
  assert.match(trayHtml,/<strong>PowerTimer<\/strong>/);
  assert.deepEqual([...trayHtml.matchAll(/data-tray-action="([^"]+)"/g)].map(x=>x[1]),['cancel','quit']);
  assert.match(trayJs,/event.key==='Escape'/);
  assert.equal(conf.app.windows.find(w=>w.label==='tray-menu').height,242);
});


test('0.7.8 centered PowerTimer heading remains readable at 16px',()=>{
  const css=readFileSync(new URL('../ui/style.css',import.meta.url),'utf8');
  assert.match(css,/\.titlebar \.app-name\{[^}]*font-size:16px/);
  assert.match(html,/<div class="app-name" data-tauri-drag-region>PowerTimer<\/div>/);
  assert.doesNotMatch(html,/TAURI EDITION/);
});
test('0.7.8 progress bar appears only when running or warning',()=>{
  assert.match(html,/<div class="progress-track is-hidden"[^>]*id="progress-track"/);
  assert.match(js,/const active=p==='running'\|\|p==='warning';/);
  assert.match(js,/\$\('progress-track'\)\.classList\.toggle\('is-hidden',!active\)/);
  assert.match(js,/\$\('progress-track'\)\.setAttribute\('aria-valuenow',String\(Math\.round\(progress\)\)\)/);
});

test('0.8.0 RC writes config atomically and logs exit while a timer is active',()=>{
  assert.match(rust,/let temporary=path.with_extension\("ini.tmp"\)/);
  assert.match(rust,/file\.sync_all\(\)/);
  assert.match(rust,/fs::rename\(&temporary,path\)/);
  assert.match(rust,/fn log_active_exit\(/);
  assert.match(rust,/"quit"=>\{log_active_exit\(&app\);/);
});
test('0.8.0 RC handles DST ambiguity, CPU streak reset and idle wrap',()=>{
  const native=readFileSync(new URL('../src-tauri/src/native.rs',import.meta.url),'utf8');
  assert.match(rust,/LocalResult::Ambiguous\(a,b\)/);
  assert.match(rust,/fn next_clock_target\(/);
  assert.match(rust,/fn cpu_streak_step\(/);
  assert.match(rust,/fn idle_reached\(/);
  assert.match(native,/fn safe_idle_delta_ms\(/);
  assert.match(native,/if elapsed>i32::MAX as u32/);
});
test('0.8.0 RC preserves the visual UI and adds Windows audit checklist',()=>{
  const checklist=readFileSync(new URL('../QA_WINDOWS_RU.md',import.meta.url),'utf8');
  assert.equal(conf.app.windows[0].width,1200);
  assert.equal(conf.app.windows[0].height,680);
  assert.match(checklist,/cargo test/);
  assert.match(checklist,/Get-Process/);
  assert.match(checklist,/Тестовый режим/);
  assert.match(checklist,/Безопасность/);
});

test('0.8.0 RC fixed-time warnings do not extend the scheduled deadline',()=>{
  assert.match(rust,/fn scheduled_warning_deadline\(/);
  assert.match(rust,/plan\.deadline\.and_then\(\|end\|scheduled_warning_deadline/);
  assert.match(rust,/fixed_warning\.unwrap_or\(now\+Duration::from_secs/);
  assert.match(rust,/fn scheduled_warning_does_not_extend_clock_or_interval\(/);
});

test('0.8.0 RC native close path does not retain the mutex while logging exit',()=>{
  assert.match(rust,/let to_tray=\{engine\.core\.lock\(\)\.unwrap\(\)\.config\.minimize_to_tray\};/);
  assert.match(rust,/else\{log_active_exit\(app\); \}/);
});

test('RC2 has continuous progress interpolation with reduced-motion support',()=>{
  const css=readFileSync(new URL('../ui/style.css',import.meta.url),'utf8');
  assert.match(css,/\.progress-track>div\{transition:width 650ms linear/);
  assert.match(css,/@media \(prefers-reduced-motion:reduce\)\{\.progress-track>div\{transition:none!important/);
  assert.match(js,/\$\('progress'\)\.style\.width=progress\+'%'/);
});
test('RC2 warning never unconditionally sets Rust progress to 100',()=>{
  assert.doesNotMatch(rust,/core\.phase="warning"[^;]*;core\.pill="СЕЙЧАС"[^;]*;core\.progress=100\.0/);
  assert.match(rust,/core\.progress=warning_progress\(now,&plan\)/);
  assert.match(rust,/core\.progress=fixed_deadline_progress\(now,end,plan\.total_seconds\)/);
  assert.match(rust,/core\.progress=condition_running_progress\(elapsed,plan\.total_seconds,plan\.config\.warning_seconds\)/);
  assert.match(rust,/core\.progress=condition_running_progress\(idle\.unwrap_or\(0\.0\),secs,plan\.config\.warning_seconds\)/);
  assert.match(rust,/fn fixed_deadline_warning_keeps_counting_without_jump/);
  assert.match(rust,/fn condition_warning_continues_from_ninety_to_hundred/);
});


test('RC3 monochrome surfaces with violet rose and gold accents',()=>{
  const css=readFileSync(new URL('../ui/style.css',import.meta.url),'utf8');
  const trayCss=readFileSync(new URL('../ui/tray.css',import.meta.url),'utf8');
  assert.match(css,/--bg:#090a0c/);
  assert.match(css,/--surface:#121316/);
  for(const accent of ['--violet:#ae8bf5','--pink:#efa0ca','--gold:#f0c77f'])assert.ok(css.includes(accent));
  assert.match(css,/\.panel\{background:#141519;border-color:#303137\}/); // Earlier opaque fallback
  assert.match(css,/\.footer-panel\{background:#16171b;border-color:#34353c\}/);
  assert.match(trayCss,/\.tray-shell\{background:#111216;border-color:#303138\}/);
  assert.equal(conf.app.windows[0].backgroundColor,'#090a0c');
});
test('RC3 launch button keeps icon and label as centered readable content',()=>{
  const css=readFileSync(new URL('../ui/style.css',import.meta.url),'utf8');
  assert.match(html,/<button[^>]+id="start-btn"[^>]*><span class="button-content"><svg[^>]*><use href="#i-play"/);
  assert.match(html,/<span class="button-label">Запустить таймер<\/span>/);
  assert.match(css,/#start-btn \.button-content\{display:inline-flex;align-items:center;justify-content:center;gap:10px/);
  assert.match(css,/\.btn-primary\{[^}]*font-size:14px/);
  assert.match(css,/\.btn-primary \.ico\{width:19px;height:19px/);
});


test('RC3 mono-glass uses translucent graphite panels and does not risk WebView2 blur flicker',()=>{
  const css=readFileSync(new URL('../ui/style.css',import.meta.url),'utf8');
  const trayCss=readFileSync(new URL('../ui/tray.css',import.meta.url),'utf8');
  assert.match(css,/--glass-panel:rgba\(20,21,24,\.74\)/);
  assert.match(css,/\.panel\{\s*background:linear-gradient\(155deg,var\(--glass-panel\),var\(--glass-panel-deep\)\)/);
  assert.match(css,/\.titlebar\{background:rgba\(7,8,11,\.80\)/);
  assert.match(css,/\.footer-panel\{\s*background:linear-gradient\(120deg,rgba/);
  assert.match(trayCss,/\.tray-status\{background:rgba\(40,39,47,\.54\)/);
  assert.match(css,/backdrop-filter:none;-webkit-backdrop-filter:none/);
  assert.match(trayCss,/backdrop-filter:none;-webkit-backdrop-filter:none/);
  assert.equal(conf.app.windows[0].shadow,false);
  assert.equal(conf.app.windows[0].decorations,false);
});


test('RC4 soft polish keeps calmer primary action and stronger glass surfaces',()=>{
  const css=readFileSync(new URL('../ui/style.css',import.meta.url),'utf8');
  assert.match(css,/0\.8\.0 RC4 — soft polish/);
  assert.match(css,/\.titlebar\{background:rgba\(13,14,17,.68\);backdrop-filter:blur\(14px\)/);
  assert.match(css,/\.panel\{background:linear-gradient\(180deg,rgba\(20,21,25,.78\),rgba\(15,16,19,.72\)\)/);
  assert.match(css,/\.btn-primary\{background:linear-gradient\(115deg,rgba\(96,74,154,.94\),rgba\(124,96,184,.94\)\)/);
  assert.match(css,/\.btn-secondary:disabled\{opacity:.58;background:rgba\(34,35,40,.64\);border-color:#4b4d56;color:#b6b7be\}/);
});


test('RC5 recolors only Start Timer and keeps readable text contrast',()=>{
  const css=readFileSync(new URL('../ui/style.css',import.meta.url),'utf8');
  const html=readFileSync(new URL('../ui/index.html',import.meta.url),'utf8');
  const styles=css.split('/* 0.8.0 RC5 — restrained purple ONLY for the main Start Timer button.')[1];
  assert.ok(styles);
  assert.match(styles,/#start-btn\{background:linear-gradient\(115deg,#674b9a,#785bab\)/);
  assert.match(styles,/#start-btn:not\(:disabled\):hover\{background:linear-gradient\(115deg,#7355a6,#8160b2\)/);
  assert.match(styles,/#start-btn:not\(:disabled\):active\{background:linear-gradient\(115deg,#593f8b,#6b4d9b\)/);
  assert.doesNotMatch(styles,/(?:^|\n)\.btn-primary\s*\{/);
  assert.match(html,/id="start-btn" class="btn btn-primary"/);
  const luminance=(hex)=>{
    const rgb=[1,3,5].map(i=>parseInt(hex.slice(i,i+2),16)/255);
    const v=rgb.map(x=>x<=.04045?x/12.92:((x+.055)/1.055)**2.4);
    return v[0]*.2126+v[1]*.7152+v[2]*.0722;
  };
  for(const bg of ['#674b9a','#785bab','#7355a6','#8160b2','#593f8b','#6b4d9b']){
    const ratio=(luminance('#f7f5fc')+.05)/(luminance(bg)+.05);
    assert.ok(ratio>=4.5, `Text contrast should be >=4.5:1 on ${bg}, got ${ratio}`);
  }
});
