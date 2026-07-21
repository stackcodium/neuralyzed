export type HudOptions = { mode?: "classpick" | "game" | "ended"; title?: string };
export type GameHudView = {
  hp:number;maxHp:number;floor:string;floorTitle:string;agent:string;agentTitle:string;
  weapon:string;weaponTitle:string;damage:string;damageTitle:string;range:string;rangeTitle:string;
  ammo:string;ammoTitle:string;armor:string;armorTitle:string;armorWarning?:boolean;
  xpPercent:number;xpTitle:string;level:string;credits:string;nutrition:string;nutritionTitle:string;
  nutritionWarning?:boolean;skillPoints?:string;effects?:string;
};
export function renderHudView(view:GameHudView){const hpPct=percent(view.hp,view.maxHp);return [
  `<div class="hud-vital" title="Health ${view.hp}/${view.maxHp}"><span class="hud-icon hud-icon-hp" aria-hidden="true"></span><div class="hud-bar" aria-label="Health ${hpPct}%"><span style="width:${hpPct}%"></span></div><strong>${view.hp}/${view.maxHp}</strong></div>`,
  chip("floor",view.floor,view.floorTitle),chip("agent",view.agent,view.agentTitle),chip("weapon",compactWeaponName(view.weapon),view.weaponTitle),chip("damage",view.damage,view.damageTitle),chip("range",view.range,view.rangeTitle),chip("ammo",view.ammo,view.ammoTitle),chip(view.armorWarning?"warn":"armor",view.armor,view.armorTitle),meter("xp",view.xpPercent,view.xpTitle,view.level),chip("credits",view.credits,"Credits"),chip(view.nutritionWarning?"warn":"food",view.nutrition,view.nutritionTitle),view.skillPoints?chip("skill",view.skillPoints,"Skill points available"):"",view.effects?chip("warn",view.effects,"Active effects"):""
].filter(Boolean).join("")}
export function renderPregameHud(options:HudOptions={}){const label=options.mode==="ended"?options.title??"Mission ended":"Select agent";return [
  `<div class="hud-vital hud-vital-empty"><span class="hud-icon hud-icon-hp" aria-hidden="true"></span><div class="hud-bar" aria-label="Health preview"><span style="width:100%"></span></div><strong>Ready</strong></div>`,
  chip("agent",label,"Choose a field profile"),chip("floor","F1","HQ Evidence Lockdown"),chip("damage","-","Damage updates when a weapon is wielded"),chip("range","-","Range updates when a weapon is wielded"),chip("ammo","-","Ammo depends on starting gear"),chip("weapon","Gear","Starting gear depends on selected agent")
].join("")}
function chip(kind:string,value:string,title:string){return `<span class="hud-chip hud-${kind}" title="${esc(title)}"><span class="hud-icon hud-icon-${kind}" aria-hidden="true"></span><strong>${esc(value)}</strong></span>`}
function meter(kind:string,pct:number,title:string,value:string){return `<span class="hud-chip hud-${kind}" title="${esc(title)}"><span class="hud-icon hud-icon-${kind}" aria-hidden="true"></span><span class="hud-mini"><span style="width:${pct}%"></span></span><strong>${esc(value)}</strong></span>`}
function compactWeaponName(name:string){const known:Record<string,string>={"noisy cricket":"Cricket","standard pistol":"Pistol","prototype zapper":"Zapper","series 4 de-atomizer":"S4 De-Atom.","reverberating carbonizer w/ mutate capacity":"Rev. Carbon.","tri-barrel plasma gun":"Tri-Plasma","bone spur":"Spur","stun baton":"Baton","arquillian saber":"Saber","sugar-water cannon":"Sugar Gun"};const compact=known[name.toLowerCase()]??name.replace(/^prototype\s+/i,"").replace(/^standard\s+/i,"").split(/\s+/).slice(0,2).join(" ");return compact.length>12?`${compact.slice(0,11).trimEnd()}.`:compact}
function percent(value:number,max:number){return max<=0?0:Math.max(0,Math.min(100,Math.round(value/max*100)))}
function esc(value:string){return value.replace(/[&<>"]/g,ch=>({"&":"&amp;","<":"&lt;",">":"&gt;","\"":"&quot;"})[ch]!)}
