'use strict';
window.homeState=__HOME_JSON__;
const homeBookmarks=__BOOKMARKS_JSON__,homeIcons=__HOME_ICONS__;
const homeLabels={logo:'Логотип',services:'Почта и профиль',clock:'Часы',date:'Дата',title:'Заголовок',subtitle:'Подпись',search:'Поиск',sites:'Быстрые сайты'};
const cloneHome=value=>JSON.parse(JSON.stringify(value));
let homeEditing=false,homeUndo=[],homeSelected='search',tileEditing=-1,homeDrag=null,tileDrag=null;
window.homeSend=command=>{const target='literust://home/?data='+encodeURIComponent(JSON.stringify(command));if(target.length>98000){homeStatus('Слишком много данных для одного сохранения. Сократите длинные ссылки.');return;}location.assign(target);};
function homeStatus(message){document.getElementById('home-status').textContent=message;}
const homeStyle=document.createElement('style');homeStyle.textContent=`
main.dashboard{display:grid!important;grid-template-columns:repeat(12,minmax(0,1fr))!important;gap:18px!important;max-width:1320px;margin:30px auto!important;padding:0 4vw!important;text-align:left!important}.home-widget{grid-column:span var(--span,12);min-width:0;position:relative;align-self:start}.home-widget[hidden]{display:none!important}.home-widget .clock,.home-widget .date{display:block!important}.home-widget .clock{margin:0}.home-widget #greeting,.home-widget .lead{display:block!important;margin:0;max-width:none}.home-widget .eyebrow{display:none}.home-widget .header-actions{display:flex!important;justify-content:flex-end;min-height:38px}.home-widget .workspace{display:block!important;margin:0}.home-widget .search-label{margin:0 0 10px}.home-widget .brand{min-height:38px}.home-widget .note,.home-widget .shortcut-hint{display:none}.home-widget .shortcuts{display:grid!important;grid-template-columns:repeat(auto-fill,minmax(min(100%,145px),1fr))}.home-widget .shortcut{position:relative;display:flex;min-height:72px;padding:14px;overflow:hidden}.home-widget .shortcut img{width:28px;height:28px;object-fit:contain}.tile-wrap{min-width:0;position:relative}.tile-wrap.drag-target{outline:2px solid var(--accent);border-radius:16px}.tile-wrap[draggable=true]{cursor:grab}.tile-edit{position:absolute;right:5px;top:4px;border:0;border-radius:6px;background:var(--bg);color:var(--muted);cursor:pointer}.add-tile{width:100%;min-height:72px;border:1px dashed var(--line);border-radius:16px;background:transparent;color:var(--accent);cursor:pointer;font:inherit}.add-tile strong{font-size:24px;margin-right:8px}.widget-tools{display:none;gap:6px;align-items:center;margin-bottom:10px}.home-editing .widget-tools{display:flex}.home-editing .home-widget{outline:1px dashed var(--accent);outline-offset:7px;border-radius:10px;min-height:70px}.home-editing .home-widget.selected{outline:2px solid var(--accent)}.widget-tools button{border:0;border-radius:5px;background:var(--surface);color:var(--text);padding:4px 7px;cursor:grab;font:11px Segoe UI}.widget-resize{display:none;position:absolute;right:-7px;bottom:-7px;width:15px;height:20px;border:0;border-radius:4px;background:var(--accent);cursor:ew-resize}.home-editing .widget-resize{display:block}.home-editing{padding-bottom:240px}#home-editor{position:fixed;bottom:12px;left:50%;transform:translateX(-50%);width:min(1000px,calc(100% - 24px));max-height:44vh;overflow:auto;z-index:20;background:var(--surface);border:1px solid var(--line);border-radius:16px;padding:18px;box-shadow:0 12px 40px #0006}#home-editor[hidden],#tile-dialog[hidden]{display:none!important}.home-editor-row{display:flex;align-items:center;gap:12px;flex-wrap:wrap;margin-bottom:12px}.home-editor-row label{font-size:12px;display:flex;align-items:center;gap:7px}#home-editor input[type=text]{width:190px}#home-editor input,#home-editor select,#tile-dialog input,#tile-dialog select{font:inherit;padding:8px;border:1px solid var(--line);border-radius:7px;background:var(--bg);color:var(--text)}#home-editor button,#tile-dialog button,#home-edit-toggle{font:inherit;border:1px solid var(--line);background:var(--surface);color:var(--text);border-radius:9px;padding:9px 12px;cursor:pointer}#home-status{font-size:12px;color:var(--muted);margin:4px 0}#tile-dialog{position:fixed;inset:0;margin:auto;width:min(450px,calc(100vw - 24px));height:max-content;max-height:90vh;overflow:auto;z-index:30;background:var(--surface);color:var(--text);border:1px solid var(--line);border-radius:18px;padding:24px;box-shadow:0 0 0 100vmax #0009}#tile-dialog label{display:block;margin:14px 0 6px}#tile-dialog input,#tile-dialog select{width:100%}.tile-dialog-actions{display:flex;gap:8px;margin-top:18px}footer{align-items:center;flex-wrap:wrap}#home-widgets{gap:12px}#home-widgets label{white-space:nowrap}@media(max-width:760px){main.dashboard{grid-template-columns:1fr!important;margin-top:20px!important}.home-widget{grid-column:1/-1!important}.home-widget .header-actions{justify-content:flex-start}.home-widget .header-actions>a:first-child{display:block}.widget-resize{display:none!important}.home-editor-row{gap:8px}#home-editor{padding:12px}.home-editing{padding-bottom:320px}}
`;document.head.append(homeStyle);
const main=document.querySelector('main'),oldHeader=document.querySelector('header');
const nodes={logo:[document.querySelector('.brand')],services:[document.querySelector('.header-actions')],clock:[document.getElementById('clock')],date:[document.getElementById('date')],title:[document.querySelector('.eyebrow'),document.getElementById('greeting')],subtitle:[document.querySelector('.lead')],search:[document.querySelector('.search-label'),document.getElementById('search'),document.getElementById('home-suggestions')],sites:[document.querySelector('.workspace')]};
const widgetNodes={};for(const [id,content]of Object.entries(nodes)){const widget=document.createElement('section');widget.className='home-widget';widget.dataset.widget=id;const tools=document.createElement('div');tools.className='widget-tools';const handle=document.createElement('button');handle.type='button';handle.textContent='↕ '+homeLabels[id];handle.draggable=true;handle.setAttribute('aria-label','Переместить: '+homeLabels[id]);handle.ondragstart=e=>{homeDrag=id;e.dataTransfer.setData('text/plain','widget:'+id);};handle.ondragend=()=>{homeDrag=null;};handle.onclick=()=>selectHomeWidget(id);tools.append(handle);widget.append(tools,...content.filter(Boolean));const resize=document.createElement('button');resize.className='widget-resize';resize.setAttribute('aria-label','Изменить ширину: '+homeLabels[id]);resize.onpointerdown=e=>{if(!homeEditing)return;e.preventDefault();selectHomeWidget(id);homeUndo.push(cloneHome(homeState));if(homeUndo.length>20)homeUndo.shift();const start=e.clientX,widget=homeState.widgets.find(w=>w.id===id),initial=widget.span,unit=main.clientWidth/12;resize.setPointerCapture(e.pointerId);resize.onpointermove=move=>{const span=Math.max(3,Math.min(12,Math.round(initial+(move.clientX-start)/Math.max(1,unit))));if(span!==widget.span){widget.span=span;applyHomeLayout();}};resize.onlostpointercapture=()=>{resize.onpointermove=null;resize.onlostpointercapture=null;homeStatus('Ширина изменена. Нажмите «Сохранить».');};};widget.append(resize);widget.ondragover=e=>{if(homeEditing&&homeDrag)e.preventDefault();};widget.ondrop=e=>{if(!homeEditing||!homeDrag)return;e.preventDefault();const from=homeDrag;homeDrag=null;if(from===id)return;changeHome(()=>{const items=homeState.widgets,index=items.findIndex(w=>w.id===from),[item]=items.splice(index,1);items.splice(items.findIndex(w=>w.id===id),0,item);});};widgetNodes[id]=widget;}
main.replaceChildren();main.classList.add('dashboard');oldHeader?.remove();
const editToggle=document.createElement('button');editToggle.id='home-edit-toggle';editToggle.textContent='Настроить страницу';document.querySelector('footer').append(editToggle);
const editor=document.createElement('aside');editor.id='home-editor';editor.hidden=true;editor.setAttribute('aria-label','Редактор главной страницы');editor.innerHTML=`<div class="home-editor-row"><strong>Ваше пространство</strong><button id="home-undo">Отменить изменение</button><button id="home-reset">Стандартная сетка</button><button id="home-save">Сохранить</button><button id="home-edit-close">Готово</button></div><div id="home-widgets" class="home-editor-row"></div><div class="home-editor-row"><label>Выбранный блок <select id="home-selected"></select></label><label>Ширина <input id="home-span" type="range" min="3" max="12" step="1"></label><button id="home-move-up">Раньше</button><button id="home-move-down">Позже</button><label>Заголовок <input id="home-title" type="text" maxlength="120" placeholder="Автоматическое приветствие"></label><label>Подпись <input id="home-subtitle" type="text" maxlength="240"></label></div><p id="home-status" role="status">Перетаскивайте блоки за заголовок ↕. Сетка не допускает перекрытий.</p>`;document.body.append(editor);
const tileDialog=document.createElement('section');tileDialog.id='tile-dialog';tileDialog.hidden=true;tileDialog.setAttribute('role','dialog');tileDialog.setAttribute('aria-modal','true');tileDialog.setAttribute('aria-labelledby','tile-dialog-title');tileDialog.innerHTML=`<h2 id="tile-dialog-title">Быстрый сайт</h2><label for="tile-bookmark">Выбрать из закладок</label><select id="tile-bookmark"><option value="">Ввести вручную</option></select><label for="tile-title">Название</label><input id="tile-title" maxlength="80"><label for="tile-url">Адрес сайта</label><input id="tile-url" type="url" maxlength="2048" placeholder="https://example.com"><p id="tile-error" role="alert"></p><div class="tile-dialog-actions"><button id="tile-save">Сохранить</button><button id="tile-delete">Удалить</button><button id="tile-cancel">Отмена</button></div>`;document.body.append(tileDialog);
for(const [i,b]of homeBookmarks.entries()){const option=document.createElement('option');option.value=String(i);option.textContent=b.title||b.url;document.getElementById('tile-bookmark').append(option);}
function applyHomeLayout(){
  let next=main.firstElementChild;
  for(const item of homeState.widgets){
    const widget=widgetNodes[item.id];
    if(!widget)continue;
    widget.hidden=item.hidden;
    if(widget.style.getPropertyValue('--span')!==String(item.span))widget.style.setProperty('--span',item.span);
    widget.classList.toggle('selected',item.id===homeSelected&&homeEditing);
    if(widget!==next)main.insertBefore(widget,next);
    next=widget.nextElementSibling;
  }
  const subtitle=document.querySelector('.lead');
  if(subtitle.textContent!==homeState.subtitle_text)subtitle.textContent=homeState.subtitle_text;
  document.getElementById('home-span').value=homeState.widgets.find(w=>w.id===homeSelected)?.span||12;
  document.getElementById('home-undo').disabled=!homeUndo.length;
  const clockMode=homeState.widgets.filter(item=>['clock','date','title'].includes(item.id)&&!item.hidden)
    .map(item=>item.id).sort().join(',')+'\n'+homeState.title_text;
  if(clockMode!==lastClockMode){
    lastClockMode=clockMode;
    window.scheduleHomeClock?.();
  }
}
let lastClockMode='';
const widgetControls=new Map();
function renderHomeControls(){
  const visibility=document.getElementById('home-widgets');
  const select=document.getElementById('home-selected');
  let nextLabel=visibility.firstElementChild;
  let nextOption=select.firstElementChild;
  for(const widget of homeState.widgets){
    let control=widgetControls.get(widget.id);
    if(!control){
      const label=document.createElement('label');
      const check=document.createElement('input');
      check.type='checkbox';
      check.onchange=()=>changeHome(()=>{
        const live=homeState.widgets.find(item=>item.id===widget.id);
        if(live)live.hidden=!check.checked;
      });
      label.append(check,document.createTextNode(homeLabels[widget.id]));
      const option=new Option(homeLabels[widget.id],widget.id);
      control={label,check,option};
      widgetControls.set(widget.id,control);
    }
    control.check.checked=!widget.hidden;
    if(control.label!==nextLabel)visibility.insertBefore(control.label,nextLabel);
    if(control.option!==nextOption)select.insertBefore(control.option,nextOption);
    nextLabel=control.label.nextElementSibling;
    nextOption=control.option.nextElementSibling;
  }
  select.value=homeSelected;
  for(const [id,value] of [['home-title',homeState.title_text],['home-subtitle',homeState.subtitle_text]]){
    const input=document.getElementById(id);
    if(document.activeElement!==input&&input.value!==value)input.value=value;
  }
}
function selectHomeWidget(id){homeSelected=id;document.getElementById('home-selected').value=id;applyHomeLayout();}
function changeHome(fn){homeUndo.push(cloneHome(homeState));if(homeUndo.length>20)homeUndo.shift();fn();applyHomeLayout();renderHomeControls();homeStatus('Есть изменения. Нажмите «Сохранить».');}
function saveHome(){window.homeSend({action:'save_home',value:homeState});homeStatus('Изменения отправлены на сохранение.');}
window.homeSync=(value,persisted=false)=>{
  if(!homeEditing&&JSON.stringify(value)!==JSON.stringify(homeState)){
    homeState=cloneHome(value);
    applyHomeLayout();renderHomeTiles();renderHomeControls();
    if(!tileDialog.hidden)closeTile();
  }
  homeStatus(persisted?'Сохранено в локальном профиле.':'Применено в текущем окне.');
};
function createTileRow(tile,nav){
  const wrap=document.createElement('div');
  wrap.className='tile-wrap';
  wrap.draggable=true;
  wrap.ondragstart=event=>{
    tileDrag=wrap.tile;
    event.dataTransfer.effectAllowed='move';
    event.dataTransfer.setData('application/x-literust-tile',String(wrap.tileIndex));
  };
  wrap.ondragend=()=>{tileDrag=null;wrap.classList.remove('drag-target');};
  wrap.ondragover=event=>{
    if(!tileDrag||tileDrag===wrap.tile)return;
    event.preventDefault();
    event.stopPropagation();
    event.dataTransfer.dropEffect='move';
    wrap.classList.add('drag-target');
  };
  wrap.ondragleave=event=>{
    if(!wrap.contains(event.relatedTarget))wrap.classList.remove('drag-target');
  };
  wrap.ondrop=event=>{
    if(!tileDrag)return;
    event.preventDefault();
    event.stopPropagation();
    wrap.classList.remove('drag-target');
    const from=homeState.tiles.indexOf(tileDrag);
    tileDrag=null;
    if(from>=0&&from!==wrap.tileIndex)moveTile(from,wrap.tileIndex);
  };
  const link=document.createElement('a');
  link.className='shortcut';
  link.draggable=false;
  link.href=tile.url;
  link.title=tile.title;
  link.setAttribute('aria-keyshortcuts','Shift+ArrowLeft Shift+ArrowRight');
  link.onkeydown=event=>{
    if(!event.shiftKey||!['ArrowLeft','ArrowRight'].includes(event.key))return;
    const from=wrap.tileIndex;
    const to=from+(event.key==='ArrowLeft'?-1:1);
    if(to<0||to>=homeState.tiles.length)return;
    event.preventDefault();
    moveTile(from,to);
    nav.children[to]?.querySelector('a').focus();
  };
  const icon=document.createElement('span');
  icon.className='shortcut-icon';
  icon.textContent=Array.from(tile.title)[0]||'L';
  let image=homeIcons['site:'+tile.url];
  if(!image){
    try{
      const url=new URL(tile.url);
      if(url.protocol==='https:')image=url.origin+'/favicon.ico';
    }catch{}
  }
  if(image){
    const img=document.createElement('img');
    img.alt='';
    img.draggable=false;
    img.loading='lazy';
    img.referrerPolicy='no-referrer';
    img.onerror=()=>img.remove();
    img.onload=()=>icon.replaceChildren(img);
    img.src=image;
    icon.append(img);
  }
  const label=document.createElement('span');
  label.className='shortcut-label';
  label.textContent=tile.title;
  link.append(icon,label);
  const edit=document.createElement('button');
  edit.className='tile-edit';
  edit.textContent='⋯';
  edit.setAttribute('aria-label','Изменить '+tile.title);
  edit.onclick=()=>openTile(wrap.tileIndex);
  wrap.append(link,edit);
  return wrap;
}
let tilesInitialized=false;
function renderHomeTiles(){
  const nav=document.querySelector('.shortcuts');
  if(!tilesInitialized){tilesInitialized=true;nav.replaceChildren();}
  const previous=new Map(Array.from(nav.querySelectorAll('.tile-wrap'),row=>[row.tileKey,row]));
  const occurrences=new Map();
  let next=nav.firstElementChild;
  for(const [index,tile] of homeState.tiles.entries()){
    const base=JSON.stringify([tile.url,tile.title]);
    const occurrence=occurrences.get(base)||0;
    occurrences.set(base,occurrence+1);
    const key=base+':'+occurrence;
    const row=previous.get(key)||createTileRow(tile,nav);
    previous.delete(key);
    row.tileKey=key;
    row.tileIndex=index;
    row.tile=tile;
    if(row!==next)nav.insertBefore(row,next);
    next=row.nextElementSibling;
  }
  for(const row of previous.values())row.remove();
  let plus=document.getElementById('add-quick-site');
  if(!plus){
    plus=document.createElement('button');
    plus.className='add-tile';
    plus.id='add-quick-site';
    plus.textContent='+ Добавить сайт';
    plus.onclick=()=>openTile(-1);
    nav.append(plus);
  }
  plus.disabled=homeState.tiles.length>=24;
}
function moveTile(from,to){
  changeHome(()=>{
    const [tile]=homeState.tiles.splice(from,1);
    homeState.tiles.splice(to,0,tile);
  });
  renderHomeTiles();
  if(!homeEditing)saveHome();
}
function openTile(index){tileEditing=index;tileDialog.hidden=false;const tile=homeState.tiles[index];document.getElementById('tile-title').value=tile?.title||'';document.getElementById('tile-url').value=tile?.url||'';document.getElementById('tile-bookmark').value='';document.getElementById('tile-error').textContent='';document.getElementById('tile-delete').hidden=index<0;document.getElementById('tile-title').focus();}
function closeTile(){tileDialog.hidden=true;document.getElementById('add-quick-site').focus();}
document.getElementById('tile-bookmark').onchange=e=>{const b=homeBookmarks[Number(e.target.value)];if(e.target.value!==''&&b){document.getElementById('tile-title').value=b.title||b.url;document.getElementById('tile-url').value=b.url;}};
document.getElementById('tile-save').onclick=()=>{const title=document.getElementById('tile-title').value.trim(),input=document.getElementById('tile-url').value.trim();let url;try{url=new URL(input.includes('://')?input:'https://'+input);if(!['http:','https:'].includes(url.protocol)||url.username||url.password||url.href.length>2048||!title)throw Error();}catch{document.getElementById('tile-error').textContent='Введите название и HTTP(S)-адрес без логина и пароля.';return;}changeHome(()=>{const tile={title,url:url.href};if(tileEditing<0)homeState.tiles.push(tile);else homeState.tiles[tileEditing]=tile;});renderHomeTiles();if(!homeEditing)saveHome();closeTile();};
document.getElementById('tile-delete').onclick=()=>{changeHome(()=>homeState.tiles.splice(tileEditing,1));renderHomeTiles();if(!homeEditing)saveHome();closeTile();};document.getElementById('tile-cancel').onclick=closeTile;
editToggle.onclick=()=>{homeEditing=!homeEditing;editor.hidden=!homeEditing;document.body.classList.toggle('home-editing',homeEditing);applyHomeLayout();renderHomeControls();};document.getElementById('home-edit-close').onclick=()=>{saveHome();homeEditing=false;editor.hidden=true;document.body.classList.remove('home-editing');applyHomeLayout();};document.getElementById('home-save').onclick=saveHome;
document.getElementById('home-undo').onclick=()=>{if(homeUndo.length){homeState=homeUndo.pop();applyHomeLayout();renderHomeControls();renderHomeTiles();homeStatus('Последнее изменение отменено. Сохраните результат.');}};
document.getElementById('home-reset').onclick=()=>changeHome(()=>{homeState.widgets=Object.keys(homeLabels).map(id=>({id,span:['logo','services','clock','date'].includes(id)?6:id==='search'?7:id==='sites'?5:12,hidden:false}));homeState.title_text='';homeState.subtitle_text='Начните с вопроса. Продолжите открытием.';tick();});
document.getElementById('home-selected').onchange=e=>selectHomeWidget(e.target.value);
document.getElementById('home-span').onchange=e=>changeHome(()=>homeState.widgets.find(w=>w.id===homeSelected).span=Number(e.target.value));
for(const [id,delta]of [['home-move-up',-1],['home-move-down',1]])document.getElementById(id).onclick=()=>changeHome(()=>{const items=homeState.widgets,index=items.findIndex(w=>w.id===homeSelected),to=index+delta;if(to>=0&&to<items.length)[items[index],items[to]]=[items[to],items[index]];});
document.getElementById('home-title').onchange=e=>changeHome(()=>{homeState.title_text=e.target.value;tick();});document.getElementById('home-subtitle').onchange=e=>changeHome(()=>homeState.subtitle_text=e.target.value);
window.addEventListener('keydown',e=>{if(!tileDialog.hidden){if(e.key==='Escape'){e.preventDefault();closeTile();}if(e.key==='Tab'){const items=[...tileDialog.querySelectorAll('input,select,button')].filter(el=>!el.hidden&&!el.disabled);if(e.shiftKey&&document.activeElement===items[0]){e.preventDefault();items.at(-1).focus();}else if(!e.shiftKey&&document.activeElement===items.at(-1)){e.preventDefault();items[0].focus();}}}});
applyHomeLayout();renderHomeControls();renderHomeTiles();

const homeQuery=document.getElementById('q'),homeMatches=document.getElementById('home-suggestions');
const recentSites=__HOME_SUGGESTIONS__;
let selectedSite=-1;
function clearHomeMatches(){
  homeMatches.replaceChildren();homeMatches.hidden=true;selectedSite=-1;
  homeQuery.setAttribute('aria-expanded','false');homeQuery.removeAttribute('aria-activedescendant');
}
homeQuery.addEventListener('input',()=>{
  clearHomeMatches();
  const query=homeQuery.value.trim().toLowerCase();
  if(query.length<2)return;
  const seen=new Set();
  for(const item of recentSites){
    if(seen.has(item.url)||!item.url.toLowerCase().includes(query)&&!item.title.toLowerCase().includes(query))continue;
    seen.add(item.url);
    const result=document.createElement('a');
    result.href=item.url;result.id='home-option-'+homeMatches.childElementCount;result.tabIndex=-1;
    result.setAttribute('role','option');result.textContent=item.title||item.url;result.title=item.url;
    result.onmousedown=e=>e.preventDefault();
    result.onclick=e=>{e.preventDefault();window.homeSend({action:'navigate',value:item.url});clearHomeMatches();};
    homeMatches.append(result);
    if(homeMatches.childElementCount===6)break;
  }
  homeMatches.hidden=!homeMatches.childElementCount;
  homeQuery.setAttribute('aria-expanded',String(!homeMatches.hidden));
});
homeQuery.addEventListener('blur',clearHomeMatches);
homeQuery.addEventListener('keydown',e=>{
  if(e.isComposing||homeMatches.hidden)return;
  const count=homeMatches.childElementCount;
  if(e.key==='Escape'){e.preventDefault();clearHomeMatches();}
  if(e.key==='Enter'&&selectedSite>=0){e.preventDefault();homeMatches.children[selectedSite].click();}
  if(e.key==='ArrowDown'||e.key==='ArrowUp'){
    e.preventDefault();
    selectedSite=selectedSite<0?(e.key==='ArrowDown'?0:count-1):(selectedSite+(e.key==='ArrowDown'?1:count-1))%count;
    [...homeMatches.children].forEach((option,i)=>option.setAttribute('aria-selected',String(i===selectedSite)));
    homeQuery.setAttribute('aria-activedescendant',homeMatches.children[selectedSite].id);
  }
});