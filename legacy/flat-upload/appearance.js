// Local-only appearance catalog. No fonts, images, network calls or extra WebViews.
(() => {
  const palettes=[['Шалфей',155],['Океан',205],['Лаванда',265],['Роза',335],['Янтарь',35],['Мята',175],['Индиго',235],['Коралл',12],['Лайм',85],['Орхидея',295],['Графит',215]];
  const modes=['Ночь','Тёмная','Светлая','Бумага','Контраст'];
  window.appearanceDesigns=['Орбита','Минимализм','Студия','Классика','Терминал'];
  window.appearanceThemes=palettes.flatMap(([name,h],p)=>modes.map((mode,m)=>({name:`${name} · ${mode}`,h,m,s:p===10?6:24})));
  let style, lastKey;
  window.applyAppearance=({design=0,theme=0}={})=>{
    design=Number.isInteger(design)&&design>=0&&design<5?design:0;
    const t=window.appearanceThemes[theme]||window.appearanceThemes[0];
    const key=design+':'+theme;
    if(key===lastKey)return;
    lastKey=key;
    const light=t.m===2||t.m===3, contrast=t.m===4;
    const color=(s,l)=>`hsl(${t.h} ${s}% ${l}%)`;
    const bg=light?color(t.s,t.m===3?94:97):contrast?'#000':color(t.s,t.m===0?7:13);
    const surface=light?color(t.s,91):contrast?'#080808':color(t.s,t.m===0?13:20);
    const text=light?'#17212b':'#f2f5fa',muted=light?'#475569':'#b6c1cf';
    const accent=light?color(65,30):color(65,76),line=light?color(16,65):color(16,40);
    const radius=[14,5,22,2,0][design];
    document.documentElement.dataset.design=String(design);
    document.documentElement.dataset.theme=String(theme);
    if(!style){style=document.createElement('style');document.head.append(style);}
    style.textContent=`
      :root{color-scheme:${light?'light':'dark'};--bg:${bg};--surface:${surface};--text:${text};--muted:${muted};--accent:${accent};--line:${line};--radius:${radius}px}
      html,body{background:var(--bg);color:var(--text)}
      body{font-family:${design===4?'Consolas,monospace':'Segoe UI,Arial,sans-serif'}}
      .tab,.address-wrap,#menu,form,.entry,#filter,.symbol,.theme-choice,.design-choice{background:var(--surface);border-color:var(--line);color:var(--text);border-radius:var(--radius)}
      .tab{border-radius:var(--radius) var(--radius) 0 0}.tab.active{background:var(--bg);border-color:var(--accent);color:var(--text);box-shadow:inset 0 3px var(--accent)}
      .tabs{border-color:var(--line)}.btn,.new,.tab-close,#menu button,#address,input,#filter{color:var(--text)}
      .btn:hover,.new:hover,#menu button:hover,a.entry:hover{background:var(--surface);color:var(--accent)}
      .address-wrap:focus-within,form:focus-within{outline:2px solid var(--accent);outline-offset:1px}
      p,footer,small,.empty,.mark{color:var(--muted)}.secure,.eyebrow,.symbol,.tag,.btn.active{color:var(--accent)}
      input::placeholder,#address::placeholder{color:var(--muted)}.favicon,form button{background:var(--accent);color:var(--bg)}
      .symbol{border-color:var(--accent)}form,.entry{box-shadow:none}.entry:hover{border-color:var(--accent)}
      button:focus-visible,a:focus-visible,select:focus-visible{outline:2px solid var(--accent);outline-offset:2px}
      ${design===1?'.symbol{display:none}h1{font-weight:400}.tab{border:0}.address-wrap{border:0;border-bottom:1px solid var(--line)}':''}
      ${design===2?'.tabs{gap:10px}.tab{border-radius:18px;margin-bottom:3px;height:32px}.address-wrap{border-radius:24px}h1{letter-spacing:-2px}.symbol{transform:rotate(-8deg)}':''}
      ${design===3?'.tab{border:1px solid var(--line)}.controls{border-bottom:1px solid var(--line)}h1{font-family:Georgia,serif}main{text-align:left}':''}
      ${design===4?'h1{letter-spacing:2px}.symbol{border-style:dashed}.tab.active{box-shadow:inset 0 -3px var(--accent)}.entry{border-style:dashed}':''}
    `;
  };
})();

window.liteIcons={};window.liteIcon=name=>{const custom=window.liteIcons[name];if(custom)return '<img class="ui-icon custom-icon" alt="" src="'+custom+'">';const paths={"back": "M19 12H5m7-7-7 7 7 7", "forward": "M5 12h14m-7-7 7 7-7 7", "reload": "M20 7v5h-5M4 17v-5h5M6.1 6a8 8 0 0 1 13 2M4.9 16a8 8 0 0 0 13 2", "home": "m3 10 9-7 9 7v10H3Zm6 10v-7h6v7", "bookmark": "m12 3 2.8 5.7 6.2.9-4.5 4.4 1.1 6.2-5.6-3-5.6 3 1.1-6.2L3 9.6l6.2-.9Z", "bookmarks": "M5 3h14v18l-7-5-7 5Z", "history": "M3 4v5h5M3 9a9 9 0 1 1 0 6m9-8v5l3 2", "downloads": "M12 3v12m-5-5 5 5 5-5M4 16v5h16v-5", "key": "M14 7a4 4 0 1 0 0 8 4 4 0 0 0 0-8Zm-4 5H3v5m3-5v3", "more": "M5 12h.01M12 12h.01M19 12h.01", "settings": "M9 3h6l1 3 3 1 2 5-2 5-3 1-1 3H9l-1-3-3-1-2-5 2-5 3-1ZM15 12a3 3 0 1 0-6 0 3 3 0 0 0 6 0", "appearance": "M12 3a9 9 0 1 0 0 18V3Z", "shield": "m12 3 8 3v6c0 5-8 9-8 9s-8-4-8-9V6Z", "globe": "M21 12a9 9 0 1 0-18 0 9 9 0 0 0 18 0ZM3 12h18M12 3c5 5 5 13 0 18-5-5-5-13 0-18Z", "lock": "M6 10h12v11H6ZM8 10V7a4 4 0 0 1 8 0v3", "info": "M12 11v6m0-10v.01M21 12a9 9 0 1 0-18 0 9 9 0 0 0 18 0Z", "warning": "m12 3 10 18H2ZM12 9v5m0 3v.01", "close": "m6 6 12 12M6 18 18 6", "plus": "M12 4v16M4 12h16", "power": "M12 2v10M6 5a9 9 0 1 0 12 0", "chevron": "m9 5 7 7-7 7", "user": "M16 7a4 4 0 1 0-8 0 4 4 0 0 0 8 0ZM4 21v-3a8 8 0 0 1 16 0v3", "search": "M16 10a6 6 0 1 0-12 0 6 6 0 0 0 12 0Zm-2 4 7 7"};return '<svg class="ui-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="'+(paths[name]||paths.globe)+'"/></svg>';};
window.refreshLiteIcons=()=>document.querySelectorAll('[data-icon]').forEach(el=>el.innerHTML=window.liteIcon(el.dataset.icon));
window.setCustomIcons=icons=>{window.liteIcons=icons||{};window.refreshLiteIcons();if(typeof render==='function')render();};

window.customizationDefaults={animations:true,compact_tabs:false,show_clock:true,show_greeting:true,wallpaper_dim:55,accent:null,font_size:13,corner_radius:12,shortcut_count:10,show_shortcuts:true,show_services:true,home_layout:0,wallpaper_contain:false,wallpaper_blur:0,clock_24:true};
window.applyCustomization=value=>{
 const p={...window.customizationDefaults,...value};
 let style=document.getElementById('customization-style');if(!style){style=document.createElement('style');style.id='customization-style';document.head.append(style);}
 const number=(v,min,max,fallback)=>Number.isFinite(Number(v))?Math.max(min,Math.min(max,Number(v))):fallback;
 const font=number(p.font_size,12,16,13),radius=number(p.corner_radius,0,24,12);
 const hasAccent=Number.isInteger(p.accent)&&p.accent>=0&&p.accent<=0xffffff;
 let accent='',ink='';
 if(hasAccent){
   const channels=[(p.accent>>16)&255,(p.accent>>8)&255,p.accent&255].map(v=>{v/=255;return v<=.04045?v/12.92:((v+.055)/1.055)**2.4;});
   const luminance=.2126*channels[0]+.7152*channels[1]+.0722*channels[2];
   const text=1.05/(luminance+.05)>(luminance+.05)/.0592?'#ffffff':'#111827';
   accent='--accent:#'+p.accent.toString(16).padStart(6,'0')+'!important;--accent-ink:'+text+';';
   ink='.search-submit,.brand-mark{color:var(--accent-ink)!important}';
 }
 style.textContent=`:root{${accent}}${ink}.tab,.address-wrap,.hub-panel article,.shortcut,.sign-in,.customize,form{border-radius:${radius}px!important}.tab,.controls input,.hub-panel label,.hub-panel input,.hub-panel select,.hub-panel button,.shortcut-label,.lead{font-size:${font}px!important}`;
};
