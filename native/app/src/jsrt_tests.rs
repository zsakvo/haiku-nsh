//! Exercises the isolated VMs while the NSH JavaScript engine renders and handles events.
use haiku_jsrt::Runtime;
use nsh_engine::{Bundle, EngineConfig};
use nsh_testing::Headless;
use serde_json::json;
use std::{
    io::{Read, Write},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

#[test]
fn jsrt_r1_r4_r5_r7_concurrent_with_nsh_ui_vm() {
    let mut app=Headless::start(EngineConfig::new(Bundle::source(r#"
      import {mount,View,Text,signal} from 'nsh';
      mount(()=>{const n=signal(0);return View({onPress:()=>n.set(n()+1),get children(){return Text({get children(){return `UI ${n()}`;}});}});});
    "#.into()))).unwrap();
    app.wait_for("UI 0").unwrap();
    let finished = Arc::new(AtomicBool::new(false));
    let complete = finished.clone();
    let thread = std::thread::spawn(move || {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
            let server=std::net::TcpListener::bind("127.0.0.1:0").unwrap();let url=format!("http://{}",server.local_addr().unwrap());
            let serving=std::thread::spawn(move||{for stream in server.incoming().take(3){let mut stream=stream.unwrap();let mut request=[0;4096];let _=stream.read(&mut request);let _=stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n42");}});
            let a=Runtime::spawn().unwrap();let b=Runtime::spawn().unwrap();
            for vm in [&a,&b] {vm.load_module("test.js","globalThis.pluginOnly=true; export default { async http(url,ctx){return Number(await (await ctx.http(url)).text());},wait(_,ctx){return new Promise(resolve=>ctx.signal.addEventListener('abort',()=>resolve(0)));},echo(value){return value;} };").await.unwrap();}
            let cancel=a.start_call("default.wait",json!(null),json!({}),Duration::from_secs(3)).unwrap();
            let abort=cancel.abort.clone();
            let trigger=tokio::spawn(async move{tokio::time::sleep(Duration::from_millis(30)).await;abort.abort();});
            let url_json=json!(url).to_string();
            let (cancelled,a1,a2,b1,b2)=tokio::join!(cancel.result(),a.call("default.http",&url_json,Duration::from_secs(3)),a.call("default.echo","21",Duration::from_secs(3)),b.call("default.http",&url_json,Duration::from_secs(3)),b.call("default.http",&url_json,Duration::from_secs(3)));
            trigger.await.unwrap();assert_eq!(cancelled.unwrap_err().name,"AbortError");assert_eq!(a2.unwrap(),json!(21));for value in [a1,b1,b2]{assert_eq!(value.unwrap(),json!(42));}
            a.unload().await.unwrap();a.shutdown().unwrap();b.shutdown().unwrap();serving.join().unwrap();
        });
        complete.store(true, Ordering::Release);
    });
    app.press("UI 0").unwrap();
    app.wait_for("UI 1").unwrap();
    app.wait_until(
        "plugin work and responsive NSH",
        Duration::from_secs(10),
        |_| finished.load(Ordering::Acquire),
    )
    .unwrap();
    thread.join().unwrap();
    // A further UI event after all VM creation/destruction confirms the engine remains live.
    app.press("UI 1").unwrap();
    app.wait_for("UI 2").unwrap();
}
