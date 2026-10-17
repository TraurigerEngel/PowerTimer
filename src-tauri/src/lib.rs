#![cfg(target_os = "windows")]
mod native;

use chrono::{DateTime, Local, LocalResult, NaiveTime, TimeZone, Duration as ChronoDuration};
use serde::{Deserialize, Serialize};
use std::{fs, io, path::PathBuf, sync::Mutex, thread, time::{Duration, Instant}};
use tauri::{Emitter, Manager, State};
use tauri::tray::{TrayIconBuilder, TrayIconEvent, MouseButton, MouseButtonState};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    action: String,
    trigger: String,
    hours: u32,
    minutes: u32,
    seconds: u32,
    target_time: String,
    idle_minutes: u32,
    cpu_threshold: u32,
    cpu_seconds: u32,
    warning_seconds: u32,
    test_mode: bool,
    minimize_to_tray: bool,
    logging: bool,
}
impl Default for Config {
    fn default() -> Self {
        Self { action: "shutdown".into(), trigger: "interval".into(), hours:0, minutes:30,
            seconds:0, target_time:"23:00:00".into(), idle_minutes:30, cpu_threshold:10,
            cpu_seconds:60, warning_seconds:30, test_mode:true, minimize_to_tray:true, logging:true }
    }
}
fn valid_action(a:&str)->bool { matches!(a,"shutdown"|"restart"|"lock"|"logoff") }
fn valid_trigger(t:&str)->bool { matches!(t,"interval"|"clock"|"idle"|"cpu") }
fn parse_clock(raw:&str)->Option<NaiveTime> {
    // Explicit clock parsing, rejecting 25:00 and untrusted extra separators.
    let parts:Vec<&str> = raw.split(':').collect();
    if !(parts.len()==2 || parts.len()==3) || parts.iter().any(|p|p.is_empty() || p.len()>2 || !p.bytes().all(|c|c.is_ascii_digit())) {return None;}
    let h = parts[0].parse::<u32>().ok()?;
    let m = parts[1].parse::<u32>().ok()?;
    let s = if parts.len()==3 { parts[2].parse::<u32>().ok()? } else { 0 };
    NaiveTime::from_hms_opt(h,m,s)
}
fn validate(c:&Config)->Result<(),String> {
    if !valid_action(&c.action) || !valid_trigger(&c.trigger) { return Err("Неизвестное действие или условие".into()); }
    if c.hours>999 || c.minutes>59 || c.seconds>59 || !(1..=1440).contains(&c.idle_minutes)
        || !(1..=99).contains(&c.cpu_threshold) || !(5..=3600).contains(&c.cpu_seconds)
        || c.warning_seconds>300 || parse_clock(&c.target_time).is_none() {
            return Err("Проверьте числовые значения и время (00:00–23:59:59)".into());
    }
    if c.trigger=="interval" && c.hours*3600+c.minutes*60+c.seconds==0 {
        return Err("Интервал должен быть больше нуля".into());
    }
    Ok(())
}
fn action_name(a:&str)->&'static str {
    match a {"shutdown"=>"Выключение компьютера", "restart"=>"Перезагрузка компьютера", "lock"=>"Блокировка компьютера", _=>"Выход из учётной записи"}
}

fn get_config_path()->PathBuf {
    let exe_dir = std::env::current_exe().ok().and_then(|p|p.parent().map(|x|x.to_owned()))
        .unwrap_or_else(||PathBuf::from("."));
    // Portable by default. Installed builds in Program Files fall back to APPDATA.
    let candidate=exe_dir.join(".powertimer-write-test");
    if fs::write(&candidate,b"").is_ok() {let _=fs::remove_file(&candidate);return exe_dir;}
    let appdata=std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or(exe_dir);
    // Keep the previous APPDATA folder to preserve portable/fallback settings after the rename.
    let folder=appdata.join("PowerTimerMini");let _=fs::create_dir_all(&folder);
    folder
}
fn read_ini(path:&PathBuf)->Config {
    let mut c=Config::default();
    let text=match fs::read_to_string(path) {Ok(s)=>s,Err(_)=>return c};
    for line in text.trim_start_matches('\u{feff}').lines() {
        let line=line.trim();if line.starts_with('#')||line.starts_with(';'){continue;}
        let Some((key,val))=line.split_once('=') else {continue;};let v=val.trim();
        let numeric=|| v.parse::<u32>().ok();
        let boolean=|| match v.to_ascii_lowercase().as_str() {"true"=>Some(true),"false"=>Some(false),_=>None};
        match key.trim() {
            "Action"=>{c.action=match v {"Выключить компьютер"=>"shutdown","Перезагрузить компьютер"=>"restart","Заблокировать компьютер"=>"lock","Выйти из учётной записи"=>"logoff",other=>other}.to_owned();}
            "Trigger"=>{c.trigger=match v {"Через интервал"=>"interval","В указанное время"=>"clock","После бездействия"=>"idle","Когда CPU ниже порога"=>"cpu",other=>other}.to_owned();}
            "IntervalHours"=>if let Some(n)=numeric(){c.hours=n.min(999)},
            "IntervalMinutes"=>if let Some(n)=numeric(){c.minutes=n.min(59)},
            "IntervalSeconds"=>if let Some(n)=numeric(){c.seconds=n.min(59)},
            "TargetTime"=>if parse_clock(v).is_some(){c.target_time=v.into()},
            "IdleMinutes"=>if let Some(n)=numeric(){c.idle_minutes=n.clamp(1,1440)},
            "CpuThreshold"=>if let Some(n)=numeric(){c.cpu_threshold=n.clamp(1,99)},
            "CpuSeconds"=>if let Some(n)=numeric(){c.cpu_seconds=n.clamp(5,3600)},
            "WarningSeconds"=>if let Some(n)=numeric(){c.warning_seconds=n.min(300)},
            "TestMode"=>if let Some(b)=boolean(){c.test_mode=b},
            "MinimizeToTray"=>if let Some(b)=boolean(){c.minimize_to_tray=b},
            "Logging"=>if let Some(b)=boolean(){c.logging=b},
            _=>{}
        }
    }
    if !valid_action(&c.action){c.action="shutdown".into()};
    if !valid_trigger(&c.trigger){c.trigger="interval".into()};
    c
}
fn save_ini(path:&PathBuf,c:&Config)->Result<(),String> {
    let a=match c.action.as_str(){"shutdown"=>"Выключить компьютер","restart"=>"Перезагрузить компьютер","lock"=>"Заблокировать компьютер",_=>"Выйти из учётной записи"};
    let t=match c.trigger.as_str(){"interval"=>"Через интервал","clock"=>"В указанное время","idle"=>"После бездействия",_=>"Когда CPU ниже порога"};
    let content=format!("# PowerTimer v0.8.0 RC3 portable settings\nAction={a}\nTrigger={t}\nIntervalHours={}\nIntervalMinutes={}\nIntervalSeconds={}\nTargetTime={}\nIdleMinutes={}\nCpuThreshold={}\nCpuSeconds={}\nWarningSeconds={}\nTestMode={}\nMinimizeToTray={}\nLogging={}\n",
        c.hours,c.minutes,c.seconds,c.target_time,c.idle_minutes,c.cpu_threshold,c.cpu_seconds,c.warning_seconds,c.test_mode,c.minimize_to_tray,c.logging);
    // Write and flush next to the destination, then atomically replace it.
    // A crash while writing must not truncate the last valid config.ini.
    let temporary=path.with_extension("ini.tmp");
    let result=(|| -> io::Result<()> {
        use io::Write;
        let mut file=fs::File::create(&temporary)?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
        drop(file); // Windows needs the handle closed before rename.
        fs::rename(&temporary,path)?;
        Ok(())
    })();
    if result.is_err(){let _=fs::remove_file(&temporary);}
    result.map_err(|e|format!("Не удалось сохранить настройки: {e}"))
}
fn log_event(dir:&PathBuf, enabled:bool,message:&str) {
    if !enabled{return;}
    let path=dir.join("Logs"); if fs::create_dir_all(&path).is_err(){return;}
    use std::io::Write;
    if let Ok(mut f)=fs::OpenOptions::new().create(true).append(true).open(path.join("PowerTimerMini.log")){
        let _=writeln!(f,"{}  {}",Local::now().format("%Y-%m-%d %H:%M:%S"),message);
    }
}
#[derive(Clone,Serialize)]
#[serde(rename_all="camelCase")]
struct Snapshot {
    config:Config,phase:String,status:String,detail:String,pill:String,progress:f64,
    cpu:Option<f64>,idle_seconds:Option<f64>,now:String,warning_remaining:u32,remaining_seconds:Option<u64>,
    config_directory:String
}
struct Plan {
    config:Config,
    deadline:Option<Instant>,
    target_label:String,
    total_seconds:f64,
    cpu_below_since:Option<Instant>,
    warning_deadline:Option<Instant>
}
struct Core {
    config:Config,
    plan:Option<Plan>,
    phase:String,status:String,detail:String,pill:String,progress:f64,
    cpu:Option<f64>,idle_seconds:Option<f64>,cpu_previous:Option<(u64,u64,u64)>,
    warning_remaining:u32
}
impl Core {
    fn snapshot(&self,dir:&PathBuf)->Snapshot {
        let now=Instant::now();
        let remaining_seconds=self.plan.as_ref().and_then(|plan| {
            if self.phase=="warning" {return Some(self.warning_remaining as u64);}
            match plan.config.trigger.as_str() {
                "interval"|"clock"=>plan.deadline.map(|deadline| deadline.saturating_duration_since(now).as_secs_f64().ceil() as u64),
                "idle"=>self.idle_seconds.map(|idle| (plan.total_seconds-idle).max(0.0).ceil() as u64),
                "cpu"=>plan.cpu_below_since.map(|since| (plan.total_seconds-since.elapsed().as_secs_f64()).max(0.0).ceil() as u64),
                _=>None
            }
        });
        Snapshot {
            config:self.config.clone(),phase:self.phase.clone(),status:self.status.clone(),detail:self.detail.clone(),
            pill:self.pill.clone(),progress:self.progress,cpu:self.cpu,idle_seconds:self.idle_seconds,
            now:Local::now().format("%H:%M:%S").to_string(),warning_remaining:self.warning_remaining,
            remaining_seconds,config_directory:dir.to_string_lossy().into_owned() }
    }
    fn ready(&mut self,status:&str) {
        self.plan=None;self.phase="ready".into();self.status=status.into();
        self.detail="Выберите действие и условие, затем запустите таймер".into();self.pill="ГОТОВО".into();
        self.progress=0.0;self.warning_remaining=0;
    }
}
struct Engine {dir:PathBuf,core:Mutex<Core>}
fn broadcast(app:&tauri::AppHandle){let engine=app.state::<Engine>();let snap=engine.core.lock().unwrap().snapshot(&engine.dir);let _=app.emit("timer-update",snap);}
fn show_window(app:&tauri::AppHandle){
    if let Some(menu)=app.get_webview_window("tray-menu"){let _=menu.hide();}
    if let Some(w)=app.get_webview_window("main") {let _=w.show();let _=w.unminimize();let _=w.set_focus();}
}
// A separately themed tray popup replaces the OS-drawn light context menu.
// Position is calculated in physical pixels and clamped to the monitor containing the cursor.
fn toggle_tray_popup(app:&tauri::AppHandle, px:f64, py:f64) {
    let Some(window)=app.get_webview_window("tray-menu") else {return;};
    if window.is_visible().unwrap_or(false){let _=window.hide();return;}
    let pos=tauri::PhysicalPosition::new(px.round() as i32,py.round() as i32);
    let Ok(size)=window.outer_size() else {return;};
    let width=size.width as i32;let height=size.height as i32;
    let monitors=app.available_monitors().unwrap_or_default();
    let monitor=monitors.iter().find(|m| {
        let o=m.position();let d=m.size();
        pos.x>=o.x && pos.x<o.x+d.width as i32 && pos.y>=o.y && pos.y<o.y+d.height as i32
    }).or_else(||monitors.first());
    let (x,y)=if let Some(m)=monitor {
        let origin=m.position();let bounds=m.size();
        let min_x=origin.x+8;let max_x=(origin.x+bounds.width as i32-width-8).max(min_x);
        let min_y=origin.y+8;let max_y=(origin.y+bounds.height as i32-height-8).max(min_y);
        let preferred_y=if pos.y-height-10>=min_y{pos.y-height-10}else{pos.y+10};
        ((pos.x-width+36).clamp(min_x,max_x),preferred_y.clamp(min_y,max_y))
    }else {(pos.x-width+36,pos.y-height-10)};
    let _=window.set_position(tauri::PhysicalPosition::new(x,y));
    let _=window.show();let _=window.set_focus();
}
fn log_active_exit(app:&tauri::AppHandle) {
    let engine=app.state::<Engine>();
    let core=engine.core.lock().unwrap();
    if core.plan.is_some() {
        log_event(&engine.dir,core.config.logging,"Выход из приложения: активный таймер остановлен");
    }
}
#[tauri::command]
fn tray_menu_action(app:tauri::AppHandle, action:String)->Result<(),String> {
    // Whitelist actions; never accept a shell command or arbitrary action string.
    match action.as_str() {
        "cancel"=>{let result=cancel_internal(&app);if result.is_ok(){if let Some(w)=app.get_webview_window("tray-menu"){let _=w.hide();}}result},
        "hide"=>{if let Some(w)=app.get_webview_window("tray-menu"){let _=w.hide();}Ok(())},
        "quit"=>{log_active_exit(&app);app.exit(0);Ok(())},
        _=>Err("Неизвестное действие меню".into()),
    }
}

fn perform(app:&tauri::AppHandle, config:Config) {
    let engine=app.state::<Engine>();
    let result=if config.test_mode {Ok(())} else {native::power_action(&config.action)};
    let mut core=engine.core.lock().unwrap();
    match result {
        Ok(())=>{
            log_event(&engine.dir,config.logging,&format!("Срабатывание: {}; тестовый режим: {}",action_name(&config.action),config.test_mode));
            core.ready(if config.test_mode {"Тест завершён. Системное действие не выполнялось."}else{"Системная команда передана Windows."});
            core.detail=if config.test_mode {format!("Было бы выполнено: {}",action_name(&config.action))} else {action_name(&config.action).into()};
            core.phase="completed".into();core.pill=if config.test_mode {"ТЕСТ".into()} else {"ГОТОВО".into()};
        }
        Err(e)=>{
            log_event(&engine.dir,config.logging,&format!("Ошибка: {e}"));core.ready("Не удалось выполнить системное действие");
            core.detail=e;core.phase="error".into();core.pill="ОШИБКА".into();
        }
    }
    drop(core);broadcast(app);show_window(app);
}
// Return the earliest future occurrence, including the second occurrence of a
// repeated hour during the autumn DST transition. Invalid spring hours are skipped.
fn next_future_occurrence<Tz:TimeZone>(now:&DateTime<Tz>, result:LocalResult<DateTime<Tz>>)->Option<DateTime<Tz>> {
    let after=now.timestamp_millis();
    match result {
        LocalResult::Single(candidate) if candidate.timestamp_millis()>after=>Some(candidate),
        LocalResult::Ambiguous(a,b)=>[a,b].into_iter()
            .filter(|candidate|candidate.timestamp_millis()>after)
            .min_by_key(|candidate|candidate.timestamp_millis()),
        _=>None,
    }
}
fn next_clock_target(now_local:DateTime<Local>, time:NaiveTime)->Result<DateTime<Local>,String> {
    let mut day=now_local.date_naive();
    // Bounded search guards against a pathological local timezone configuration.
    for _ in 0..=366 {
        if let Some(target)=next_future_occurrence(&now_local,Local.from_local_datetime(&day.and_time(time))){return Ok(target);}
        day=day.succ_opt().ok_or("Ошибка вычисления даты")?;
    }
    Err("Не удалось найти допустимое время запуска".into())
}
// CPU must remain BELOW the threshold for the entire specified duration.
// Missing readings and spikes reset the accumulated streak.
fn cpu_streak_step(since:&mut Option<Instant>,now:Instant,cpu:Option<f64>,threshold:u32,period:f64)->(f64,bool) {
    match cpu {
        Some(v) if v.is_finite() && v < threshold as f64 => {
            let start=since.get_or_insert(now);
            let elapsed=now.saturating_duration_since(*start).as_secs_f64();
            (elapsed,elapsed>=period)
        }
        _=>{*since=None;(0.0,false)}
    }
}
fn idle_reached(idle:Option<f64>,required:f64)->bool {
    idle.is_some_and(|value|value.is_finite() && value>=required)
}
// Fixed-time triggers warn BEFORE their deadline. If Windows wakes up after
// the due time, give the user a short safe grace period rather than acting instantly.
fn scheduled_warning_deadline(now:Instant,end:Instant,warning_seconds:u32)->Option<Instant> {
    if warning_seconds==0 || end.saturating_duration_since(now)>Duration::from_secs(warning_seconds as u64){return None;}
    Some(if now>=end {now+Duration::from_secs(5)}else{end})
}
// Presentation only. Never infer the action deadline from this percentage.
// Preserve the final 10% for the confirmation window on idle/CPU triggers,
// since unlike fixed deadlines their warning starts after the condition fires.
const CONDITIONAL_RUNNING_MAX_PERCENT:f64=90.0;
fn percent_done(done:f64,total:f64,limit:f64)->f64 {
    if !done.is_finite() || !total.is_finite() || total<=0.0{return 0.0;}
    (limit*done/total).clamp(0.0,limit)
}
fn fixed_deadline_progress(now:Instant,end:Instant,total:f64)->f64 {
    let remaining=end.saturating_duration_since(now).as_secs_f64();
    percent_done(total-remaining,total,100.0)
}
fn condition_running_progress(done:f64,total:f64,warning_seconds:u32)->f64 {
    let limit=if warning_seconds>0 {CONDITIONAL_RUNNING_MAX_PERCENT}else{100.0};
    percent_done(done,total,limit)
}
fn warning_progress(now:Instant,plan:&Plan)->f64 {
    let Some(end)=plan.warning_deadline else{return 0.0;};
    if let Some(deadline)=plan.deadline {
        // Normal fixed-time warning is within the original countdown, NOT extra time.
        if end==deadline {return fixed_deadline_progress(now,deadline,plan.total_seconds);}
        // Overdue after sleep: the scheduled moment really has passed; show 100%
        // during the safety grace period, not a fictitious restarted countdown.
        return 100.0;
    }
    // For conditions that can't be forecast, animate the reserved 90..100%.
    let total=plan.config.warning_seconds.max(1) as f64;
    let remaining=end.saturating_duration_since(now).as_secs_f64();
    (CONDITIONAL_RUNNING_MAX_PERCENT+
        (100.0-CONDITIONAL_RUNNING_MAX_PERCENT)*(1.0-remaining/total)).clamp(CONDITIONAL_RUNNING_MAX_PERCENT,100.0)
}

fn start_plan(config:&Config)->Result<Plan,String>{
    validate(config)?;
    let now=Instant::now();let mut deadline=None;let mut target_label=String::new();let total_seconds;
    if config.trigger=="interval" {
        let secs=config.hours as u64*3600+config.minutes as u64*60+config.seconds as u64;
        deadline=Some(now+Duration::from_secs(secs));total_seconds=secs as f64;
        target_label=(Local::now()+ChronoDuration::seconds(secs as i64)).format("%d.%m.%Y %H:%M:%S").to_string();
    }else if config.trigger=="clock" {
        let time=parse_clock(&config.target_time).ok_or("Неверное время")?;
        let now_local=Local::now();
        let target=next_clock_target(now_local,time)?;
        let secs=(target-now_local).num_milliseconds().max(1) as f64/1000.0;
        deadline=Some(now+Duration::from_secs_f64(secs));total_seconds=secs;
        target_label=target.format("%d.%m.%Y %H:%M:%S").to_string();
    }else if config.trigger=="idle"{total_seconds=(config.idle_minutes*60)as f64;}
    else {total_seconds=config.cpu_seconds as f64;}
    Ok(Plan {config:config.clone(),deadline,target_label,total_seconds,cpu_below_since:None,warning_deadline:None})
}

#[tauri::command]
fn get_snapshot(engine:State<'_,Engine>)->Snapshot {engine.core.lock().unwrap().snapshot(&engine.dir)}
#[tauri::command]
fn update_settings(engine:State<'_,Engine>, config:Config)->Result<Snapshot,String> {
    validate(&config)?;
    let mut core=engine.core.lock().unwrap();
    if core.plan.is_some() {
        // Permit tray/logging toggles, but freeze all scheduling and safety fields.
        let old=&core.config;
        let immutable_same=old.action==config.action && old.trigger==config.trigger
            && old.hours==config.hours && old.minutes==config.minutes && old.seconds==config.seconds
            && old.target_time==config.target_time && old.idle_minutes==config.idle_minutes
            && old.cpu_threshold==config.cpu_threshold && old.cpu_seconds==config.cpu_seconds
            && old.warning_seconds==config.warning_seconds && old.test_mode==config.test_mode;
        if !immutable_same{return Err("Нельзя изменять параметры запущенного таймера".into());}
    }
    save_ini(&engine.dir.join("config.ini"),&config)?;
    core.config=config;
    Ok(core.snapshot(&engine.dir))
}
#[tauri::command]
fn start_timer(app:tauri::AppHandle,engine:State<'_,Engine>,config:Config)->Result<Snapshot,String>{
    let plan=start_plan(&config)?;
    let mut core=engine.core.lock().unwrap();
    if core.plan.is_some(){return Err("Таймер уже активен".into());}
    save_ini(&engine.dir.join("config.ini"),&config)?;
    log_event(&engine.dir,config.logging,&format!("Таймер запущен: {} / {}",action_name(&config.action),config.trigger));
    core.config=config;core.plan=Some(plan);core.phase="running".into();core.pill="АКТИВЕН".into();
    core.status="Таймер запущен".into();core.detail="Ожидание выполнения условия".into();core.progress=0.0;
    let snap=core.snapshot(&engine.dir);drop(core);broadcast(&app);Ok(snap)
}
fn cancel_internal(app:&tauri::AppHandle)->Result<(),String>{
    let engine=app.state::<Engine>();let mut core=engine.core.lock().unwrap();
    if core.plan.is_none(){return Ok(());}
    log_event(&engine.dir,core.config.logging,"Таймер отменён пользователем");
    core.ready("Таймер отменён");drop(core);broadcast(app);Ok(())
}
#[tauri::command]
fn cancel_timer(app:tauri::AppHandle)->Result<(),String>{cancel_internal(&app)}
fn extend_internal(app:&tauri::AppHandle)->Result<(),String>{
    let engine=app.state::<Engine>();let mut core=engine.core.lock().unwrap();
    let plan=core.plan.as_mut().ok_or("Таймер не запущен")?;
    if plan.warning_deadline.is_some() || plan.deadline.is_none(){return Err("+30 минут доступно только для активного таймера по времени".into());}
    plan.deadline=plan.deadline.map(|d|d+Duration::from_secs(1800));
    plan.total_seconds+=1800.0;
    if !plan.target_label.is_empty(){
        // The active deadline is monotonic; label is display-only.
        plan.target_label=(Local::now()+ChronoDuration::seconds(plan.deadline.unwrap().saturating_duration_since(Instant::now()).as_secs() as i64)).format("%d.%m.%Y %H:%M:%S").to_string();
    }
    log_event(&engine.dir,core.config.logging,"К таймеру добавлено 30 минут");
    drop(core);broadcast(app);Ok(())
}
#[tauri::command]
fn extend_timer(app:tauri::AppHandle)->Result<(),String>{extend_internal(&app)}
#[tauri::command]
fn resolve_warning(app:tauri::AppHandle,proceed:bool)->Result<(),String>{
    let engine=app.state::<Engine>();let mut core=engine.core.lock().unwrap();
    let plan=core.plan.as_ref().ok_or("Таймер не активен")?;
    if plan.warning_deadline.is_none(){return Err("Предупреждение не активно".into());}
    let config=plan.config.clone();
    core.ready(if proceed {"Выполнение действия"} else {"Действие отменено"});
    if !proceed{log_event(&engine.dir,config.logging,"Отменено в предупреждении");}
    drop(core);
    if proceed{perform(&app,config);}else{broadcast(&app);}
    Ok(())
}
#[tauri::command]
fn minimize_main(app:tauri::AppHandle)->Result<(),String>{
    let engine=app.state::<Engine>();let to_tray=engine.core.lock().unwrap().config.minimize_to_tray;
    if let Some(w)=app.get_webview_window("main"){
        if to_tray{w.hide().map_err(|e|e.to_string())}else{w.minimize().map_err(|e|e.to_string())}
    }else{Err("Окно не найдено".into())}
}
#[tauri::command]
fn close_main(app:tauri::AppHandle)->Result<(),String>{
    let engine=app.state::<Engine>();let to_tray=engine.core.lock().unwrap().config.minimize_to_tray;
    if to_tray {if let Some(w)=app.get_webview_window("main"){w.hide().map_err(|e|e.to_string())}else{Err("Окно не найдено".into())}}
    else {log_active_exit(&app);app.exit(0);Ok(())}
}

fn tick(app:&tauri::AppHandle) {
    let engine=app.state::<Engine>();
    let cpu_times=native::cpu_times();let idle=native::idle_seconds();
    let mut wake=false;let mut action=None::<Config>;
    let mut core=engine.core.lock().unwrap();
    core.idle_seconds=idle;
    if let Some((i,k,u))=cpu_times {
        if let Some((pi,pk,pu))=core.cpu_previous {
            let di=i.saturating_sub(pi);let dt=k.saturating_sub(pk).saturating_add(u.saturating_sub(pu));
            core.cpu=if dt>0 {Some(((dt.saturating_sub(di)) as f64*100.0/dt as f64).clamp(0.0,100.0))}else{None};
        }
        core.cpu_previous=Some((i,k,u));
    } else {core.cpu=None;core.cpu_previous=None;}
    let cpu=core.cpu;
    if let Some(mut plan)=core.plan.take(){
        let now=Instant::now();let mut triggered=false;
        if let Some(warning_deadline)=plan.warning_deadline {
            let remaining=warning_deadline.saturating_duration_since(now).as_secs_f64();
            core.warning_remaining=remaining.ceil() as u32;
            core.phase="warning".into();core.pill="СЕЙЧАС".into();core.progress=warning_progress(now,&plan);
            core.status=format!("{} — через {} сек.",action_name(&plan.config.action),core.warning_remaining);
            core.detail="Можно выполнить сейчас или отменить".into();
            if now>=warning_deadline {action=Some(plan.config.clone());}
            else{core.plan=Some(plan);}
        }else {
            match plan.config.trigger.as_str(){
                "interval"|"clock"=>{
                    let end=plan.deadline.unwrap();let left=end.saturating_duration_since(now).as_secs_f64();
                    core.progress=fixed_deadline_progress(now,end,plan.total_seconds);
                    core.status=format!("{} через {}",action_name(&plan.config.action),format_secs(left));
                    core.detail=format!("Цель: {}",plan.target_label);
                    core.pill=if left<=10.0{"СЕЙЧАС"}else if left<=60.0{"СКОРО"}else{"АКТИВЕН"}.into();
                    triggered=now>=end;
                }
                "idle"=>{
                    let secs=plan.total_seconds;
                    core.progress=condition_running_progress(idle.unwrap_or(0.0),secs,plan.config.warning_seconds);
                    core.status=if let Some(v)=idle {format!("Бездействие: {} из {}",format_secs(v),format_secs(secs))}else{"Не удалось получить время бездействия".into()};
                    core.detail=action_name(&plan.config.action).into();core.pill="АКТИВЕН".into();
                    triggered=idle_reached(idle,secs);
                }
                "cpu"=>{
                    let (elapsed,met)=cpu_streak_step(&mut plan.cpu_below_since,now,cpu,plan.config.cpu_threshold,plan.total_seconds);
                    triggered=met;
                    core.progress=condition_running_progress(elapsed,plan.total_seconds,plan.config.warning_seconds);
                    core.status=match cpu {
                        Some(v) if v.is_finite() && v<plan.config.cpu_threshold as f64=>
                            format!("CPU {:.0}% ниже {}% — {} из {}",v,plan.config.cpu_threshold,format_secs(elapsed),format_secs(plan.total_seconds)),
                        Some(v) if v.is_finite()=>format!("CPU {:.0}% — нужен уровень ниже {}%",v,plan.config.cpu_threshold),
                        _=>"Ожидание корректного замера CPU".into()
                    };
                    core.detail=action_name(&plan.config.action).into();core.pill="АКТИВЕН".into();
                }
                _=>{}
            }
            // Interval/clock deadlines are the *execution* time, not the start
            // of an extra warning delay. Idle/CPU thresholds cannot be forecast:
            // for those conditions the warning begins once the threshold is met.
            let fixed_warning=plan.deadline.and_then(|end|scheduled_warning_deadline(now,end,plan.config.warning_seconds));
            if fixed_warning.is_some() || triggered {
                if plan.config.warning_seconds==0 {action=Some(plan.config.clone());}
                else {
                    let end=fixed_warning.unwrap_or(now+Duration::from_secs(plan.config.warning_seconds as u64));
                    plan.warning_deadline=Some(end);
                    core.warning_remaining=end.saturating_duration_since(now).as_secs_f64().ceil() as u32;
                    core.phase="warning".into();core.pill="СЕЙЧАС".into();
                    core.status=format!("Сейчас: {}",action_name(&plan.config.action));
                    core.detail="Подтвердите действие или дождитесь окончания предупреждения".into();
                    core.plan=Some(plan);wake=true;
                }
            }else{core.plan=Some(plan);}
        }
    }
    let snap=core.snapshot(&engine.dir);drop(core);
    let _=app.emit("timer-update",snap);
    if wake{show_window(app);}
    if let Some(config)=action {perform(app,config);}
}
fn format_secs(sec:f64)->String {
    let total=sec.max(0.0).ceil() as u64;
    let h=total/3600;let m=(total%3600)/60;let s=total%60;
    if h>0{format!("{h} ч {m:02} мин {s:02} с")}
    else if m>0{format!("{m} мин {s:02} с")}else{format!("{s} с")}
}

pub fn run(){
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app,_,_|show_window(app)))
        .setup(|app| {
            let dir=get_config_path();let config=read_ini(&dir.join("config.ini"));
            log_event(&dir,config.logging,"Запуск PowerTimer 0.8.0 RC3 (Tauri)");
            app.manage(Engine {dir,core:Mutex::new(Core {
                config,plan:None,phase:"ready".into(),status:"Таймер не запущен".into(),
                detail:"Выберите действие и условие, затем запустите таймер".into(),
                pill:"ГОТОВО".into(),progress:0.0,cpu:None,idle_seconds:None,
                cpu_previous:None,warning_remaining:0
            })});
            let mut tray=TrayIconBuilder::new().tooltip("PowerTimer")
                .show_menu_on_left_click(false)
                .on_tray_icon_event(|tray,event| {
                    match event {
                        TrayIconEvent::Click {button:MouseButton::Right,button_state:MouseButtonState::Up,position,..} => {
                            toggle_tray_popup(tray.app_handle(),position.x,position.y);
                        }
                        TrayIconEvent::Click {button:MouseButton::Left,button_state:MouseButtonState::Up,..} | TrayIconEvent::DoubleClick {button:MouseButton::Left,..} => {
                            show_window(tray.app_handle());
                        }
                        _=>{}
                    }
                });
            if let Some(icon)=app.default_window_icon(){tray=tray.icon(icon.clone());}
            tray.build(app)?;
            let handle=app.handle().clone();
            thread::Builder::new().name("power-timer-engine".into()).spawn(move ||loop {
                thread::sleep(Duration::from_millis(500));
                tick(&handle);
            }).map_err(|e|io::Error::other(format!("Ошибка запуска таймера: {e}")))?;
            Ok(())
        })
        .on_window_event(|window,event|{
            if window.label()=="tray-menu" {
                match event {
                    tauri::WindowEvent::Focused(false)=>{let _=window.hide();}
                    tauri::WindowEvent::CloseRequested{api,..}=>{api.prevent_close();let _=window.hide();}
                    _=>{}
                }
                return;
            }
            if let tauri::WindowEvent::CloseRequested{api,..}=event {
                let app=window.app_handle();
                let engine=app.state::<Engine>();
                // Release the core mutex before logging the exit in the other branch.
                let to_tray={engine.core.lock().unwrap().config.minimize_to_tray};
                if to_tray {
                    api.prevent_close();let _=window.hide();
                }else{log_active_exit(app); }
            }
        })
        .invoke_handler(tauri::generate_handler![get_snapshot,update_settings,start_timer,cancel_timer,extend_timer,resolve_warning,minimize_main,close_main,tray_menu_action])
        .run(tauri::generate_context!()).expect("Не удалось запустить Tauri");
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn clock_check(){assert!(parse_clock("23:00:00").is_some());assert!(parse_clock("25:00").is_none());assert!(parse_clock("12:99").is_none());}
    #[test] fn safe_defaults(){let c=Config::default();assert!(c.test_mode);assert!(valid_action(&c.action));assert!(validate(&c).is_ok());}
    #[test] fn preserve_legacy(){let dir=std::env::temp_dir().join(format!("ptm-test-{}",std::process::id()));let _=fs::create_dir_all(&dir);let f=dir.join("config.ini");
        fs::write(&f,"Action=Перезагрузить компьютер\nTrigger=После бездействия\nTestMode=False\n").unwrap();let c=read_ini(&f);
        assert_eq!(c.action,"restart");assert_eq!(c.trigger,"idle");assert!(!c.test_mode);let _=fs::remove_dir_all(dir);
    }
    #[test] fn zero_interval_is_invalid(){let mut c=Config::default();c.minutes=0;assert!(validate(&c).is_err());}
    #[test] fn config_persistence_round_trip_atomic(){
        let base=std::env::temp_dir().join(format!("ptm-config-{}-{:?}",std::process::id(),std::thread::current().id()));
        fs::create_dir_all(&base).unwrap();let path=base.join("config.ini");
        let mut config=Config::default();config.minutes=15;config.warning_seconds=5;
        save_ini(&path,&config).unwrap();assert_eq!(read_ini(&path).minutes,15);
        config.minutes=45;save_ini(&path,&config).unwrap();
        assert_eq!(read_ini(&path).minutes,45);
        assert!(!path.with_extension("ini.tmp").exists());
        fs::remove_dir_all(base).unwrap();
    }
    #[test] fn validation_boundaries(){
        let mut c=Config::default();
        c.hours=999;c.minutes=59;c.seconds=59;c.warning_seconds=300;assert!(validate(&c).is_ok());
        c.hours=1000;assert!(validate(&c).is_err());c.hours=0;
        c.target_time="25:00".into();assert!(validate(&c).is_err());c.target_time="23:00".into();
        c.trigger="not-an-option".into();assert!(validate(&c).is_err());
    }
    #[test] fn interval_deadline_and_clock_future(){
        let mut c=Config::default();c.minutes=0;c.seconds=2;
        let plan=start_plan(&c).unwrap();assert!(plan.deadline.is_some());assert_eq!(plan.total_seconds,2.0);
        c.trigger="clock".into();let clock=start_plan(&c).unwrap();assert!(clock.deadline.is_some());assert!(clock.total_seconds>0.0);
    }
    #[test] fn idle_condition_requires_valid_reading(){
        assert!(!idle_reached(None,60.0));assert!(!idle_reached(Some(59.999),60.0));
        assert!(idle_reached(Some(60.0),60.0));assert!(!idle_reached(Some(f64::NAN),60.0));
    }
    #[test] fn cpu_streak_requires_continuous_low_load(){
        let begin=Instant::now();let mut since=None;
        assert_eq!(cpu_streak_step(&mut since,begin,Some(5.0),10,5.0),(0.0,false));
        assert!(!cpu_streak_step(&mut since,begin+Duration::from_secs(4),Some(9.0),10,5.0).1);
        assert!(!cpu_streak_step(&mut since,begin+Duration::from_secs(5),Some(10.0),10,5.0).1);
        assert!(since.is_none());
        assert!(!cpu_streak_step(&mut since,begin+Duration::from_secs(6),Some(5.0),10,5.0).1);
        assert!(cpu_streak_step(&mut since,begin+Duration::from_secs(11),Some(5.0),10,5.0).1);
        assert!(!cpu_streak_step(&mut since,begin+Duration::from_secs(12),None,10,5.0).1);
        assert!(since.is_none());
    }
    #[test] fn ambiguous_clock_prefers_second_future_occurrence(){
        use chrono::{Utc, TimeZone};
        let early=Utc.with_ymd_and_hms(2026,11,1,1,30,0).single().unwrap();
        let late=early+ChronoDuration::hours(1);
        let between=early+ChronoDuration::minutes(15);
        assert_eq!(next_future_occurrence(&between,LocalResult::Ambiguous(early,late)),Some(late));
        assert!(next_future_occurrence(&late,LocalResult::Ambiguous(early,late)).is_none());
        assert!(next_future_occurrence(&between,LocalResult::None).is_none());
    }
    #[test] fn cancellation_clears_active_timer(){
        let mut c=Core{config:Config::default(),plan:Some(start_plan(&Config::default()).unwrap()),
            phase:"running".into(),status:"active".into(),detail:String::new(),pill:String::new(),
            progress:50.0,cpu:None,idle_seconds:None,cpu_previous:None,warning_remaining:5};
        c.ready("Отменён");assert!(c.plan.is_none());assert_eq!(c.progress,0.0);
        assert_eq!(c.warning_remaining,0);assert_eq!(c.phase,"ready");
    }

    #[test] fn fixed_deadline_warning_keeps_counting_without_jump(){
        let now=Instant::now();let due=now+Duration::from_secs(60);
        let mut cfg=Config::default();cfg.warning_seconds=30;
        let mut plan=Plan{config:cfg,deadline:Some(due),target_label:String::new(),
            total_seconds:60.0,cpu_below_since:None,warning_deadline:Some(due)};
        // At the beginning of a 30-second warning for a 60-second timer, we're 50% done.
        assert!((warning_progress(now+Duration::from_secs(30),&plan)-50.0).abs()<0.000001);
        assert!((warning_progress(now+Duration::from_secs(45),&plan)-75.0).abs()<0.000001);
        assert!((warning_progress(due,&plan)-100.0).abs()<0.000001);
        // Neither entering the modal nor extra timer ticks may force completion.
        assert!(warning_progress(now+Duration::from_secs(31),&plan)<100.0);
        plan.warning_deadline=Some(due+Duration::from_secs(5));
        assert_eq!(warning_progress(due,&plan),100.0); // overdue grace is already past due
    }
    #[test] fn condition_warning_continues_from_ninety_to_hundred(){
        let now=Instant::now();let mut cfg=Config::default();cfg.trigger="cpu".into();cfg.warning_seconds=10;
        assert_eq!(condition_running_progress(5.0,10.0,10),45.0);
        assert_eq!(condition_running_progress(10.0,10.0,10),90.0);
        assert_eq!(condition_running_progress(10.0,10.0,0),100.0);
        let plan=Plan{config:cfg,deadline:None,target_label:String::new(),total_seconds:10.0,
            cpu_below_since:None,warning_deadline:Some(now+Duration::from_secs(10))};
        assert!((warning_progress(now,&plan)-90.0).abs()<0.000001);
        assert!((warning_progress(now+Duration::from_secs(5),&plan)-95.0).abs()<0.000001);
        assert!((warning_progress(now+Duration::from_secs(10),&plan)-100.0).abs()<0.000001);
    }
    #[test] fn progress_percentage_is_bounded_and_safe(){
        assert_eq!(percent_done(f64::NAN,60.0,100.0),0.0);
        assert_eq!(percent_done(120.0,60.0,100.0),100.0);
        assert_eq!(percent_done(-3.0,60.0,100.0),0.0);
        assert_eq!(percent_done(3.0,0.0,100.0),0.0);
    }

    #[test] fn scheduled_warning_does_not_extend_clock_or_interval(){
        let now=Instant::now();let due=now+Duration::from_secs(60);
        assert!(scheduled_warning_deadline(now,due,30).is_none());
        assert_eq!(scheduled_warning_deadline(now+Duration::from_secs(30),due,30),Some(due));
        assert_eq!(scheduled_warning_deadline(now+Duration::from_secs(50),due,30),Some(due));
        assert!(scheduled_warning_deadline(now+Duration::from_secs(60),due,0).is_none());
        assert_eq!(scheduled_warning_deadline(now+Duration::from_secs(70),due,30),Some(now+Duration::from_secs(75)));
    }

}
