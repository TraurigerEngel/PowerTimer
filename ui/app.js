"use strict";
const tauri = window.__TAURI__;
const invoke = tauri?.core?.invoke;
const $ = (id) => document.getElementById(id);
const ACTIONS = Object.freeze({shutdown:'Выключение компьютера',restart:'Перезагрузка компьютера',lock:'Блокировка компьютера',logoff:'Выход из учётной записи'});
const DEFAULTS = Object.freeze({action:'shutdown',trigger:'interval',hours:0,minutes:30,seconds:0,targetTime:'23:00:00',idleMinutes:30,cpuThreshold:10,cpuSeconds:60,warningSeconds:30,testMode:true,minimizeToTray:true,logging:true});
let config={...DEFAULTS};let snapshot=null;let busy=false;let toastTimer=0;let saveTimer=0;let warningOpen=false;let warningReturnFocus=null;
function clamp(value,min,max,fallback){const n=Number(value);return Number.isFinite(n)?Math.max(min,Math.min(max,Math.trunc(n))):fallback;}
function fmtSeconds(value){const sec=Math.floor(Math.max(0,value)),h=Math.floor(sec/3600),m=Math.floor((sec%3600)/60),s=sec%60;return h?`${h} ч ${String(m).padStart(2,'0')} мин`:m?`${m} мин ${String(s).padStart(2,'0')} с`:`${s} с`;}
function fmtCountdown(value){if(value==null||!Number.isFinite(Number(value)))return '—:—:—';const sec=Math.ceil(Math.max(0,Number(value)));const hours=Math.floor(sec/3600),minutes=Math.floor(sec%3600/60),seconds=sec%60;return `${String(hours).padStart(2,'0')}:${String(minutes).padStart(2,'0')}:${String(seconds).padStart(2,'0')}`;}
function toast(message){const el=$('toast');el.textContent=String(message);el.classList.remove('is-hidden');window.clearTimeout(toastTimer);toastTimer=window.setTimeout(()=>el.classList.add('is-hidden'),4500);}
function setValue(id,value){$(id).value=String(value);}
function readNumber(id,fallback){const el=$(id);return clamp(el.value,Number(el.min),Number(el.max),fallback);}
function fromForm(){
  const next={...config,hours:readNumber('hours',config.hours),minutes:readNumber('minutes',config.minutes),seconds:readNumber('seconds',config.seconds),
    targetTime:$('target-time').value||'23:00:00',idleMinutes:readNumber('idle-minutes',config.idleMinutes),
    cpuThreshold:readNumber('cpu-threshold',config.cpuThreshold),cpuSeconds:readNumber('cpu-seconds',config.cpuSeconds),
    warningSeconds:readNumber('warning-seconds',config.warningSeconds),testMode:$('test-mode').checked,
    minimizeToTray:$('minimize-to-tray').checked,logging:$('logging').checked};
  return next;
}
function renderConfig(c){
  config={...DEFAULTS,...c};
  ['hours','minutes','seconds'].forEach(id=>setValue(id,config[id]));
  setValue('target-time',config.targetTime);setValue('idle-minutes',config.idleMinutes);setValue('cpu-threshold',config.cpuThreshold);
  setValue('cpu-seconds',config.cpuSeconds);setValue('warning-seconds',config.warningSeconds);
  $('test-mode').checked=config.testMode;$('minimize-to-tray').checked=config.minimizeToTray;$('logging').checked=config.logging;
  document.querySelectorAll('[data-action]').forEach(el=>{const active=el.dataset.action===config.action;el.classList.toggle('is-selected',active);el.setAttribute('aria-pressed',String(active));});
  document.querySelectorAll('[data-trigger]').forEach(el=>{const active=el.dataset.trigger===config.trigger;el.classList.toggle('is-selected',active);el.setAttribute('aria-pressed',String(active));});
  for(const name of ['interval','clock','idle','cpu'])$('trigger-'+name).classList.toggle('is-hidden',config.trigger!==name);
  document.querySelectorAll('[data-preset]').forEach(el=>{const value=Number(el.dataset.preset);
    const selected=value===2300?config.trigger==='clock'&&config.targetTime.startsWith('23:00'):
      config.trigger==='interval'&&(config.hours*3600+config.minutes*60+config.seconds)===value;
    el.classList.toggle('active',selected);el.setAttribute('aria-pressed',String(selected));
  });
}
function updateAvailability(s){
  const active=s && (s.phase==='running'||s.phase==='warning');
  const isWarning=s?.phase==='warning';
  // No click-while-busy races: action/trigger/preset/number controls stay frozen
  // during IPC operations, not just while the timer is running.
  document.querySelectorAll('.action-option,.trigger-option,.trigger-fields input,.trigger-fields button,[data-preset],#warning-seconds,.warning-stepper button,#test-mode').forEach(el=>el.disabled=Boolean(active)||busy);
  $('start-btn').disabled=Boolean(active)||!invoke||busy;
  $('cancel-btn').disabled=!active||busy;
  $('warning-cancel').disabled=busy;$('warning-accept').disabled=busy;
  // Tray/log toggles remain adjustable, mirroring v0.6.1.
  const wasWarning=warningOpen;
  warningOpen=Boolean(isWarning);
  $('warning-modal').classList.toggle('is-hidden',!isWarning);
  if(isWarning){
    $('warning-action').textContent=ACTIONS[s.config.action]||'Системное действие';
    $('warning-countdown').textContent=String(s.warningRemaining);
    if(!wasWarning){
      warningReturnFocus=document.activeElement;
      // Safest default is Cancel; Enter should never execute an unintended action.
      queueMicrotask(()=>{if(warningOpen)$('warning-cancel').focus({preventScroll:true});});
    }
  }else if(wasWarning){
    const previous=warningReturnFocus;warningReturnFocus=null;
    queueMicrotask(()=>{
      const dest=previous?.isConnected&&!previous.disabled?previous:$('start-btn');
      if(dest&&!dest.disabled)dest.focus({preventScroll:true});
    });
  }
}
function renderSnapshot(s){
  const previous=snapshot; snapshot=s;if(!s)return;
  const p=s.phase;
  if(!previous)renderConfig(s.config);
  $('now').textContent=s.now||'--:--:--';
  $('cpu-readout').textContent=s.cpu==null?'—':Math.round(s.cpu)+'%';
  $('idle-readout').textContent=s.idleSeconds==null?'—':fmtSeconds(s.idleSeconds);
  $('pill').textContent=s.pill||'ГОТОВО';$('pill').className='pill '+(p==='running'?'running':p==='warning'?'warning':p==='error'?'error':p==='completed'?'completed':'');
  $('status').textContent=s.status;$('detail').textContent=s.detail;
  const progress=Math.max(0,Math.min(100,Number(s.progress)||0));
  $('progress').style.width=progress+'%';
  $('progress-track').setAttribute('aria-valuenow',String(Math.round(progress)));
  const active=p==='running'||p==='warning';
  // No inactive progress rail: display it only while a timer is active.
  $('progress-track').classList.toggle('is-hidden',!active);
  $('remaining-time').classList.toggle('is-hidden',!active);
  $('remaining-label').textContent=p==='warning'?'ДО ДЕЙСТВИЯ':s.remainingSeconds==null?'ОЖИДАНИЕ':'ОСТАЛОСЬ';
  $('remaining-value').textContent=fmtCountdown(s.remainingSeconds);
  $('footer-panel').classList.toggle('is-running',p==='running');
  $('footer-panel').classList.toggle('is-warning',p==='warning');
  updateAvailability(s);
}
async function saveSettings({quiet=true}={}){
  config=fromForm();if(!invoke)return;
  try {const result=await invoke('update_settings',{config});renderSnapshot(result);}catch(e){if(!quiet)toast(String(e));}
}
function queueSave(){window.clearTimeout(saveTimer);saveTimer=window.setTimeout(()=>saveSettings(),350);}
async function perform(cmd,args={}){
  if(!invoke||busy)return;
  busy=true;updateAvailability(snapshot);
  try{await invoke(cmd,args);renderSnapshot(await invoke('get_snapshot'));}
  catch(error){toast(String(error));}
  finally{busy=false;updateAvailability(snapshot);}
}
function setupControls(){
  for(const el of document.querySelectorAll('[data-action]'))el.addEventListener('click',()=>{if(snapshot?.phase==='running')return;config={...fromForm(),action:el.dataset.action};renderConfig(config);queueSave();});
  for(const el of document.querySelectorAll('[data-trigger]'))el.addEventListener('click',()=>{if(snapshot?.phase==='running')return;config={...fromForm(),trigger:el.dataset.trigger};renderConfig(config);queueSave();});
  for(const el of document.querySelectorAll('[data-preset]'))el.addEventListener('click',()=>{
    config=fromForm();const sec=Number(el.dataset.preset);
    if(sec===2300){config.trigger='clock';config.targetTime='23:00:00';}
    else{config.trigger='interval';config.hours=Math.floor(sec/3600);config.minutes=Math.floor(sec%3600/60);config.seconds=0;}
    renderConfig(config);queueSave();
  });
  for(const el of document.querySelectorAll('.stepper')){
    const input=el.querySelector('input');const step=Number(el.dataset.step||'1');
    for(const button of el.querySelectorAll('button'))button.addEventListener('click',()=>{
      const factor=button.classList.contains('step-plus')?1:-1;
      input.value=String(clamp(Number(input.value||0)+factor*step,Number(input.min),Number(input.max),Number(input.min)));
      config=fromForm();renderConfig(config);queueSave();
    });
  }
  document.querySelectorAll('input').forEach(el=>{
    if(el.closest('.switch'))el.addEventListener('change',()=>{config=fromForm();renderConfig(config);queueSave();});
    else el.addEventListener('change',()=>{
      config=fromForm();renderConfig(config);queueSave();
    });
  });
  $('start-btn').addEventListener('click',async()=>{
    window.clearTimeout(saveTimer);
    const latest=fromForm();config=latest;renderConfig(config);await perform('start_timer',{config:latest});
  });
  $('cancel-btn').addEventListener('click',()=>perform('cancel_timer'));
  $('minimize-btn').addEventListener('click',async()=>{window.clearTimeout(saveTimer);await saveSettings();await perform('minimize_main');});
  $('close-btn').addEventListener('click',async()=>{window.clearTimeout(saveTimer);await saveSettings();await perform('close_main');});
  $('warning-cancel').addEventListener('click',()=>perform('resolve_warning',{proceed:false}));
  $('warning-accept').addEventListener('click',()=>perform('resolve_warning',{proceed:true}));
  document.addEventListener('keydown',event=>{
    if(!warningOpen)return;
    if(event.key==='Escape'){
      event.preventDefault();
      if(!event.repeat&&!busy)perform('resolve_warning',{proceed:false});
      return;
    }
    // Keep keyboard navigation inside the modal until it closes.
    const controls=[$('warning-cancel'),$('warning-accept')].filter(el=>!el.disabled);
    if(event.key==='Tab'&&controls.length){
      const idx=controls.indexOf(document.activeElement);
      const next=event.shiftKey?(idx<=0?controls.at(-1):controls[idx-1]):(idx<0||idx===controls.length-1?controls[0]:controls[idx+1]);
      event.preventDefault();next.focus({preventScroll:true});
      return;
    }
    if(event.key==='Enter'){
      if(event.repeat){event.preventDefault();return;}
      // Native Enter behavior clicks exactly the focused modal button.
      // An Enter outside the modal only focuses the safe Cancel button.
      if(!controls.includes(document.activeElement)){
        event.preventDefault();if(controls.length)controls[0].focus({preventScroll:true});
      }
    }
  });
}
async function boot(){
  setupControls();renderConfig(DEFAULTS);updateAvailability(null);
  if(!invoke){$('preview-note').classList.remove('is-hidden');return;}
  try{
    // Register before first snapshot to avoid missing the first timer tick.
    await tauri.event.listen('timer-update',event=>renderSnapshot(event.payload));
    renderSnapshot(await invoke('get_snapshot'));
    document.addEventListener('visibilitychange',async()=>{
      if(document.visibilityState==='visible'){try{renderSnapshot(await invoke('get_snapshot'));}catch{}}
    });
  }catch(error){toast('Ошибка связи с системной частью: '+String(error));}
}
window.addEventListener('DOMContentLoaded',boot);
