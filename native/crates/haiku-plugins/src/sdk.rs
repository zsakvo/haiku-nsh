//! Host-supplied `haiku` module; plugin code never constructs lifecycle state.
pub(crate) const MODULE: &str = r#"
import * as crypto from 'crypto';
export function definePlugin(definition){globalThis.__haikuDefinition=definition;return definition;}
const makeField=type=>(label,options={})=>({type,label,required:true,...options});
export const field=Object.freeze(Object.fromEntries(['text','url','secret','number','boolean','select','textarea'].map(type=>[type,makeField(type)])));
class PluginError extends Error{constructor(name,code,input){super(typeof input==='string'?input:input?.message||name);this.name=name;this.code=code;if(input&&typeof input==='object')Object.assign(this,input);}}
export class AuthRequired extends PluginError{constructor(x){super('AuthRequired','AUTH_REQUIRED',x);}}
export class AuthExpired extends PluginError{constructor(x){super('AuthExpired','AUTH_EXPIRED',x);}}
export class VipRequired extends PluginError{constructor(x){super('VipRequired','VIP_REQUIRED',x);}}
export class GeoRestricted extends PluginError{constructor(x){super('GeoRestricted','GEO_RESTRICTED',x);}}
export class NotFound extends PluginError{constructor(x){super('NotFound','NOT_FOUND',x);}}
export class RateLimited extends PluginError{constructor(x){super('RateLimited','RATE_LIMITED',x);}}
export class Upstream extends PluginError{constructor(x){super('Upstream','UPSTREAM_ERROR',x);}}
export class Expired extends PluginError{constructor(x){super('Expired','PLAYBACK_EXPIRED',x);}}
export class ParseError extends PluginError{constructor(x){super('ParseError','PARSE_ERROR',x);}}
export class Unsupported extends PluginError{constructor(x){super('Unsupported','CAPABILITY_UNSUPPORTED',x);}}
function makeContext(raw){
 const definition=globalThis.__haikuDefinition,defaults=definition.http||{},ctx={...raw,crypto};
 const cookieKey='__haikuCookies';
 const request=(method,input,options={})=>{
  const execute=async()=>{
   const url=new URL(input);for(const [key,value]of Object.entries(options.query||{}))if(value!==undefined)url.searchParams.set(key,String(value));
   const headers=new Headers(defaults.headers);new Headers(options.headers).forEach((v,k)=>headers.set(k,v));
   if(defaults.cookies){const jar=raw.storage.get(cookieKey)||[];const now=Date.now();const cookies=jar.filter(c=>c.expires>now&&(c.hostOnly?url.hostname===c.domain:url.hostname===c.domain||url.hostname.endsWith('.'+c.domain))&&url.pathname.startsWith(c.path)&&(!c.secure||url.protocol==='https:'));if(cookies.length)headers.set('cookie',cookies.map(c=>c.name+'='+c.value).join('; '));}
   let body=options.body;
   if(options.json!==undefined){body=JSON.stringify(options.json);headers.set('content-type','application/json');}
   if(options.form){body=new URLSearchParams(options.form).toString();headers.set('content-type','application/x-www-form-urlencoded');}
   const start=Date.now();const response=await raw.http(url.toString(),{method,body,headers,timeout:options.timeout??defaults.timeout??15000});
   const safe=new URL(url.toString());safe.username='';safe.password='';for(const key of safe.searchParams.keys())if(/token|cookie|password|secret|api.?key|^t$|^s$|^u$/i.test(key))safe.searchParams.set(key,'[REDACTED]');
   ctx.log.debug({method,url:safe.toString(),status:response.status,elapsedMs:Date.now()-start});
   if(defaults.cookies){let jar=raw.storage.get(cookieKey)||[];for(const line of response.headers.getSetCookie()){
    const parts=line.split(';').map(s=>s.trim()),first=parts.shift(),at=first.indexOf('=');if(at<1)continue;const name=first.slice(0,at),value=first.slice(at+1),attrs=Object.fromEntries(parts.map(s=>{const i=s.indexOf('=');return i<0?[s.toLowerCase(),true]:[s.slice(0,i).toLowerCase(),s.slice(i+1)];}));
    const domain=String(attrs.domain||url.hostname).replace(/^\./,'').toLowerCase();if(url.hostname!==domain&&!url.hostname.endsWith('.'+domain))continue;
    const path=typeof attrs.path==='string'&&attrs.path.startsWith('/')?attrs.path:url.pathname.slice(0,url.pathname.lastIndexOf('/')+1)||'/';
    const expires=attrs['max-age']!==undefined?Date.now()+Number(attrs['max-age'])*1000:attrs.expires?Date.parse(attrs.expires):Number.MAX_SAFE_INTEGER;
    jar=jar.filter(c=>!(c.name===name&&c.domain===domain&&c.path===path));if(expires>Date.now())jar.push({name,value,domain,path,expires,hostOnly:!attrs.domain,secure:!!attrs.secure});
   }raw.storage.set(cookieKey,jar);}
   if(options.throwOnError!==false&&!response.ok){if(response.status===429)throw new RateLimited({status:429,retryAfter:Number(response.headers.get('retry-after')||0)});throw new Upstream({status:response.status,message:'HTTP '+response.status});}
   return response;
  };
  let pending;const response=()=>pending||(pending=execute());return {response,text:async()=>(await response()).text(),bytes:async()=>(await response()).bytes(),json:async()=>(await response()).json()};
 };
 ctx.http=Object.freeze({get:(url,options)=>request('GET',url,options),post:(url,options)=>request('POST',url,options)});
 return Object.freeze(ctx);
}
export async function invoke(payload,raw){
 const ctx=makeContext(raw);let owner=globalThis.__haikuDefinition;const parts=payload.path.split('.');for(const key of parts.slice(0,-1))owner=owner?.[key];const fn=owner?.[parts.at(-1)];if(typeof fn!=='function')throw new Unsupported(payload.path);
 const result=await fn.call(owner,...payload.args,ctx);return result===undefined?null:result;}
"#;
pub(crate) const BRIDGE: &str = r#"
export {invoke} from 'haiku';
export function debugStorage() { return JSON.parse(__storage('dump','','null')); }
export function clearStorage() { localStorage.clear(); return null; }
"#;
