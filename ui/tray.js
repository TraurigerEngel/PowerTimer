'use strict';
const tauri=window.__TAURI__;
const invoke=tauri?.core?.invoke;
const $=id=>document.getElementById(id);
let busy=false;
let latest=null;
function duration(secs){
  if(!Number.isFinite(secs))return 'Ожидание условия';
  const n=Math.max(0,Math.ceil(secs));
  return `${String(Math.floor(n/3600)).padStart(2,'0')}:${String(Math.floor(n/60)%60).padStart(2,'0')}:${String(n%60).padStart(2,'0')}`;
}
function display(s){
  latest=s;
  const active=s?.phase==='running'||s?.phase==='warning';
  const warning=s?.phase==='warning';
  $('tray-status').textContent=active?(warning?'Выполнение скоро':s.status||'Таймер активен'):'Таймер не запущен';
  $('tray-detail').textContent=active?(s.remainingSeconds==null?'Ожидание условия':`Осталось ${duration(s.remainingSeconds)}`):'Ожидание запуска';
  $('status-indicator').className='status-indicator'+(warning?' warning':active?' running':'');
  $('tray-cancel').disabled=busy||!active;
}
async function refresh(){if(!invoke)return;try{display(await invoke('get_snapshot'));}catch{}}
async function activate(action){
  if(busy||!invoke)return;
  const element=document.querySelector(`[data-tray-action="${action}"]`);
  if(action!=='hide'&&(!element||element.disabled))return;
  busy=true;
  display(latest);
  let error=null;
  try{await invoke('tray_menu_action',{action});}
  catch(e){error=String(e);}
  finally{
    busy=false;display(latest);
    if(error){
      $('tray-status').textContent='Не удалось выполнить команду';
      $('tray-detail').textContent=error;
    }
  }
}
document.addEventListener('DOMContentLoaded',async()=>{
  for(const btn of document.querySelectorAll('[data-tray-action]')){
    btn.addEventListener('click',()=>activate(btn.dataset.trayAction));
  }
  document.addEventListener('keydown',event=>{
    if(event.key==='Escape'){event.preventDefault();activate('hide');}
  });
  window.addEventListener('focus',refresh);
  document.addEventListener('visibilitychange',()=>{if(!document.hidden)refresh();});
  if(tauri?.event?.listen){
    await tauri.event.listen('timer-update',event=>display(event.payload));
  }
  await refresh();
});
