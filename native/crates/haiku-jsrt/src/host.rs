use crate::RuntimeOptions;
use quickjs_jit_stdlib::abort::AbortSignal;
use rquickjs::{
    Class, Ctx, Exception,
    function::{Async, Func},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    io::Write,
    sync::{Arc, Mutex},
    time::Duration,
};

/// A redacted, instance-tagged console message.
#[derive(Clone, Debug)]
pub struct LogRecord {
    pub instance_id: String,
    pub level: String,
    pub message: String,
}
/// Request or response metadata, never body bytes. Sensitive headers are redacted.
#[derive(Clone, Debug)]
pub struct HttpEvent {
    pub instance_id: String,
    pub phase: String,
    pub method: String,
    pub url: String,
    pub headers: BTreeMap<String, String>,
    pub status: Option<u16>,
}
fn secret(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    [
        "authorization",
        "cookie",
        "token",
        "password",
        "secret",
        "api-key",
        "apikey",
    ]
    .iter()
    .any(|s| key.contains(s))
}
fn redact(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(object) => {
            for (key, value) in object {
                if secret(key) {
                    *value = "[REDACTED]".into();
                } else {
                    redact(value);
                }
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                redact(value);
            }
        }
        serde_json::Value::String(text) => {
            // Free-form console strings may contain header lines or URL query credentials.
            let pattern = regex::Regex::new(r#"(?i)((?:authorization|cookie|set-cookie|[\w-]*token[\w-]*|password|secret|api-key)\s*[:=]\s*)([^\r\n&,;]+)"#).unwrap();
            *text = pattern.replace_all(text, "${1}[REDACTED]").into_owned();
        }
        _ => {}
    }
}
fn http_error(error: reqwest::Error) -> String {
    if error.is_timeout() {
        "Request timed out".into()
    } else {
        error.without_url().to_string()
    }
}

fn clean(text: &str) -> String {
    let mut value = serde_json::Value::String(text.into());
    redact(&mut value);
    value.as_str().unwrap().into()
}
fn headers(map: &reqwest::header::HeaderMap) -> BTreeMap<String, String> {
    map.iter()
        .map(|(k, v)| {
            (
                k.to_string(),
                if secret(k.as_str()) {
                    "[REDACTED]".into()
                } else {
                    v.to_str().unwrap_or("").into()
                },
            )
        })
        .collect()
}
#[derive(Deserialize)]
struct RequestData {
    url: String,
    method: String,
    headers: BTreeMap<String, String>,
    body: Option<Vec<u8>>,
    timeout: u64,
    redirect: String,
}
#[derive(Serialize)]
struct ResponseData {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    url: String,
    redirected: bool,
}

pub(crate) fn install<'js>(ctx: &Ctx<'js>, options: &RuntimeOptions) -> rquickjs::Result<()> {
    ctx.globals().set(
        "__timerSleep",
        Func::from(Async(
            |delay: u64, signal: Class<'js, AbortSignal<'js>>| async move {
                let cancelled = signal.borrow().sender.subscribe();
                if signal.borrow().aborted {
                    return false;
                }
                tokio::select! {
                    _ = cancelled.recv() => false,
                    _ = tokio::time::sleep(Duration::from_millis(delay)) => true,
                }
            },
        )),
    )?;
    let path = options.storage_path.clone();
    let initial: BTreeMap<String, serde_json::Value> = match path.as_ref().filter(|p| p.exists()) {
        Some(path) => serde_json::from_slice(
            &std::fs::read(path).map_err(|e| Exception::throw_message(ctx, &e.to_string()))?,
        )
        .map_err(|e| Exception::throw_message(ctx, &e.to_string()))?,
        None => BTreeMap::new(),
    };
    let storage = Arc::new(Mutex::new(initial));
    ctx.globals().set(
        "__storage",
        Func::from(
            move |ctx: Ctx<'_>,
                  action: String,
                  key: String,
                  value: String|
                  -> rquickjs::Result<String> {
                let mut guard = storage.lock().unwrap();
                if action == "dump" {
                    return Ok(serde_json::to_string(&*guard).unwrap());
                }
                if action == "get" {
                    return Ok(guard
                        .get(&key)
                        .unwrap_or(&serde_json::Value::Null)
                        .to_string());
                }
                let mut next = guard.clone();
                match action.as_str() {
                    "set" => {
                        next.insert(
                            key,
                            serde_json::from_str(&value)
                                .map_err(|e| Exception::throw_message(&ctx, &e.to_string()))?,
                        );
                    }
                    "delete" => {
                        next.remove(&key);
                    }
                    "clear" => next.clear(),
                    _ => return Err(Exception::throw_type(&ctx, "Unknown storage operation")),
                }
                if let Some(path) = &path {
                    let save = || -> Result<(), Box<dyn std::error::Error>> {
                        let parent = path.parent().ok_or("storage requires a parent directory")?;
                        std::fs::create_dir_all(parent)?;
                        let mut temp = tempfile::NamedTempFile::new_in(parent)?;
                        temp.write_all(&serde_json::to_vec(&next)?)?;
                        temp.as_file().sync_all()?;
                        temp.persist(path)?;
                        std::fs::File::open(parent)?.sync_all()?;
                        Ok(())
                    };
                    save().map_err(|e| Exception::throw_message(&ctx, &e.to_string()))?;
                }
                *guard = next;
                Ok("null".into())
            },
        ),
    )?;
    let log = options.log.clone();
    let id = options.instance_id.clone();
    ctx.globals().set(
        "__log",
        Func::from(move |level: String, message: String| {
            if let Some(log) = &log {
                let mut value =
                    serde_json::from_str(&message).unwrap_or(serde_json::Value::String(message));
                redact(&mut value);
                log(LogRecord {
                    instance_id: id.clone(),
                    level,
                    message: value.to_string(),
                });
            }
        }),
    )?;
    let hook = options.http_hook.clone();
    let fetch_url = options.fetch_url.clone();
    let id = options.instance_id.clone();
    // Separate policies keep redirect semantics explicit, sharing connection pools per instance.
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .map_err(|e| Exception::throw_message(ctx, &e.to_string()))?;
    let direct = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| Exception::throw_message(ctx, &e.to_string()))?;
    ctx.globals().set(
        "__fetch",
        Func::from(Async(
            move |ctx: Ctx<'js>, text: String, signal: Class<'js, AbortSignal<'js>>| {
                let client = client.clone();
                let direct = direct.clone();
                let hook = hook.clone();
                let fetch_url = fetch_url.clone();
                let id = id.clone();
                async move {
                    let data: RequestData = serde_json::from_str(&text)
                        .map_err(|e| Exception::throw_type(&ctx, &e.to_string()))?;
                    let cancelled = signal.borrow().sender.subscribe();
                    if signal.borrow().aborted {
                        return Err(Exception::throw_message(&ctx, "AbortError"));
                    }
                    let method = reqwest::Method::from_bytes(data.method.as_bytes())
                        .map_err(|e| Exception::throw_type(&ctx, &e.to_string()))?;
                    let target = match &fetch_url {
                        Some(rewrite) => {
                            rewrite(&data.url).map_err(|e| Exception::throw_message(&ctx, &e))?
                        }
                        None => data.url.clone(),
                    };
                    let follow = data.redirect == "follow" && fetch_url.is_none();
                    let mut request = if follow { &client } else { &direct }
                        .request(method, &target)
                        .timeout(Duration::from_millis(data.timeout));
                    for (key, value) in &data.headers {
                        request = request.header(key, value);
                    }
                    if let Some(body) = data.body {
                        request = request.body(body);
                    }
                    let request = request
                        .build()
                        .map_err(|e| Exception::throw_type(&ctx, &e.to_string()))?;
                    if let Some(hook) = &hook {
                        hook(HttpEvent {
                            instance_id: id.clone(),
                            phase: "request".into(),
                            method: data.method.clone(),
                            url: clean(&data.url),
                            headers: headers(request.headers()),
                            status: None,
                        });
                    }
                    let operation = async {
                        let response = if follow { &client } else { &direct }
                            .execute(request)
                            .await
                            .map_err(http_error)?;
                        let status = response.status().as_u16();
                        let url = if fetch_url.is_some() {
                            data.url.clone()
                        } else {
                            response.url().to_string()
                        };
                        if let Some(hook) = &hook {
                            hook(HttpEvent {
                                instance_id: id,
                                phase: "response".into(),
                                method: data.method,
                                url: clean(&url),
                                headers: headers(response.headers()),
                                status: Some(status),
                            });
                        }
                        if data.redirect == "error" && response.status().is_redirection() {
                            return Err("Redirect rejected".to_string());
                        }
                        let headers = response
                            .headers()
                            .iter()
                            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
                            .collect();
                        // Bound native response memory independently of the QuickJS heap.
                        let mut response = response;
                        let mut body = Vec::new();
                        while let Some(chunk) = response.chunk().await.map_err(http_error)? {
                            if body.len() + chunk.len() > 16 * 1024 * 1024 {
                                return Err("Response exceeds 16 MiB".into());
                            }
                            body.extend_from_slice(&chunk);
                        }
                        Ok(ResponseData {
                            status,
                            headers,
                            body,
                            redirected: url != data.url,
                            url,
                        })
                    };
                    let result: Result<ResponseData, String> = tokio::select! {
                        _ = cancelled.recv() => Err("AbortError".into()),
                        response = operation => response,
                    };
                    serde_json::to_string(&result.map_err(|e| Exception::throw_message(&ctx, &e))?)
                        .map_err(|e| Exception::throw_message(&ctx, &e.to_string()))
                }
            },
        )),
    )?;
    ctx.eval::<(), _>(BOOTSTRAP)
}

const BOOTSTRAP: &str = r#"
(() => {
 const { Buffer } = globalThis;
 let currentSignal;
 const timers = new Map();
 let timerId = 0;
 function schedule(callback, delay, repeat, args) {
   if (typeof callback !== 'function') throw new TypeError('Timer callback must be a function');
   const id = ++timerId, controller = new AbortController(), owner = currentSignal;
   const cancel = () => { controller.abort(); timers.delete(id); };
   if (owner?.aborted) return id;
   owner?.addEventListener('abort', cancel, { once: true });
   timers.set(id, cancel);
   const ms = Math.max(1, Math.min(2147483647, Number(delay) || 0));
   (async () => {
     try {
       do {
         if (!await __timerSleep(ms, controller.signal) || !timers.has(id)) return;
         if (!repeat) timers.delete(id);
         callback(...args);
       } while (repeat && timers.has(id));
     } finally {
       timers.delete(id);
       owner?.removeEventListener('abort', cancel);
     }
   })();
   return id;
 }
 Object.assign(globalThis, {
   setTimeout: (fn, delay = 0, ...args) => schedule(fn, delay, false, args),
   setInterval: (fn, delay = 0, ...args) => schedule(fn, delay, true, args),
   clearTimeout: id => timers.get(id)?.(),
   clearInterval: id => timers.get(id)?.(),
 });
 class Headers {
   constructor(init = {}) { this._pairs = []; if (init instanceof Headers) init = init._pairs;
     for (const [k,v] of (Array.isArray(init) ? init : Object.entries(init))) this.append(k,v); }
   append(k,v) { k=String(k).toLowerCase(); v=String(v); if (!/^[!#$%&'*+.^_`|~0-9a-z-]+$/.test(k) || /[\r\n]/.test(v)) throw new TypeError('Invalid header'); this._pairs.push([k,v.trim()]); }
   set(k,v) { this.delete(k); this.append(k,v); }
   delete(k) { k=String(k).toLowerCase(); this._pairs=this._pairs.filter(([n])=>n!==k); }
   get(k) { const values=this._pairs.filter(([n])=>n===String(k).toLowerCase()).map(([,v])=>v); return values.length?values.join(', '):null; }
   has(k) { return this.get(k)!==null; }
   getSetCookie() { return this._pairs.filter(([n])=>n==='set-cookie').map(([,v])=>v); }
   *entries() { for(const k of new Set(this._pairs.map(([k])=>k))) yield [k,this.get(k)]; }
   *keys() { for(const [k] of this) yield k; } *values() { for(const [,v] of this) yield v; }
   [Symbol.iterator]() { return this.entries(); } forEach(fn,thisArg) { for(const [k,v] of this) fn.call(thisArg,v,k,this); }
 }
 const bytes = data => data == null ? Buffer.alloc(0) : typeof data==='string' ? Buffer.from(data) : data instanceof ArrayBuffer ? Buffer.from(new Uint8Array(data)) : ArrayBuffer.isView(data) ? Buffer.from(new Uint8Array(data.buffer,data.byteOffset,data.byteLength)) : (()=>{throw new TypeError('Unsupported body; serialize JSON explicitly')})();
 class Body {
   constructor(body) { this._body=bytes(body); this.bodyUsed=false; }
   _consume() { if(this.bodyUsed) throw new TypeError('Body already consumed'); this.bodyUsed=true; return this._body; }
   async text() { return this._consume().toString('utf8'); } async json() { return JSON.parse(await this.text()); }
   async bytes() { return new Uint8Array(this._consume()); } async arrayBuffer() { const b=this._consume(); return new Uint8Array(b).buffer; }
 }
 class Request extends Body {
   constructor(input,init={}) { const prior=input instanceof Request?input:null; super(init.body===undefined?prior?prior._body:null:init.body);
     this.url=prior?prior.url:String(input); this.method=String(init.method||prior?.method||'GET').toUpperCase();
     this.headers=new Headers(init.headers||prior?.headers); this.signal=init.signal||prior?.signal||currentSignal||new AbortController().signal;
     this.timeout=init.timeout??prior?.timeout??30000; this.redirect=init.redirect||prior?.redirect||'follow';
     if(!['follow','manual','error'].includes(this.redirect)) throw new TypeError('Invalid redirect policy');
     if(['GET','HEAD'].includes(this.method)&&this._body.length) throw new TypeError('GET/HEAD cannot have a body');
   }
   clone() { if(this.bodyUsed) throw new TypeError('Body already consumed'); return new Request(this); }
 }
 class Response extends Body {
   constructor(body=null,init={}) { super(body); this.status=init.status??200; this.statusText=init.statusText||''; this.headers=new Headers(init.headers); this.url=init.url||''; this.redirected=!!init.redirected; this.ok=this.status>=200&&this.status<300; }
   clone() { if(this.bodyUsed) throw new TypeError('Body already consumed'); return new Response(this._body,this); }
   static json(data,init={}) { const headers=new Headers(init.headers); if(!headers.has('content-type')) headers.set('content-type','application/json'); return new Response(JSON.stringify(data),{...init,headers}); }
 }
 const fetch = async (input,init={}) => {
   const req=new Request(input,init); if(req.signal.aborted) throw req.signal.reason;
   try { const result=JSON.parse(await __fetch(JSON.stringify({url:req.url,method:req.method,headers:Object.fromEntries(req.headers),body:['GET','HEAD'].includes(req.method)?null:Array.from(req._consume()),timeout:req.timeout,redirect:req.redirect}),req.signal));
     return new Response(Buffer.from(result.body),result);
   } catch(error) { if(req.signal.aborted) throw req.signal.reason; if(error.message==='AbortError') error.name='AbortError'; if(/timed out|timeout/i.test(error.message)) error.name='TimeoutError'; throw error; }
 };
 Object.assign(globalThis,{Headers,Request,Response,fetch});
 const console={}; for(const level of ['debug','log','info','warn','error','trace']) console[level]=(...args)=>__log(level,JSON.stringify(args));
 globalThis.console=Object.freeze(console);
 const storage=Object.freeze({get:(key)=>JSON.parse(__storage('get',String(key),'null')),set:(key,value)=>__storage('set',String(key),JSON.stringify(value)),delete:(key)=>__storage('delete',String(key),'null'),clear:()=>__storage('clear','','null'),getItem:(key)=>JSON.parse(__storage('get',String(key),'null')),setItem:(key,value)=>__storage('set',String(key),JSON.stringify(String(value))),removeItem:(key)=>__storage('delete',String(key),'null')});
 globalThis.localStorage=storage;
 globalThis.__makeContext=(data)=>{ const controller=new AbortController(); const signal=controller.signal;currentSignal=signal;
   return {controller,ctx:Object.freeze({...data,signal,http:(input,init={})=>fetch(input,{...init,signal:init.signal?AbortSignal.any([signal,init.signal]):signal}),storage,log:console})}; };
 globalThis.__describe=(object)=>{ const seen=new Set(); function shape(value,depth=0){if(typeof value==='function')return 'function'; if(value===null||typeof value!=='object')return value; if(depth>16||seen.has(value))return '[object]'; seen.add(value); const out=Array.isArray(value)?[]:{}; for(const key of Object.keys(value)){const d=Object.getOwnPropertyDescriptor(value,key);out[key]=d.get?'getter':shape(d.value,depth+1);} return out;} return JSON.stringify(shape(object)); };
})();
"#;
