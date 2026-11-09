//! macOS media session and display sleep assertion. Runs AppKit/MediaPlayer on main queue.
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Default)]
#[serde(default)]
pub struct MediaInfo {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration: f64,
    pub position: f64,
    pub playing: bool,
    /// Local cached image; HTTP/credentials are resolved by the parent.
    pub artwork_path: Option<String>,
    pub can_next: bool,
    pub can_previous: bool,
    pub favorite: bool,
}

#[cfg(target_os = "macos")]
mod mac {
    use super::*;
    use cocoa::{
        base::{id, nil, NO, YES},
        foundation::{NSSize, NSString},
    };
    use objc::{class, msg_send, sel, sel_impl};
    use std::sync::{Mutex, Once};
    static INSTALL: Once = Once::new();
    static ASSERTION: Mutex<Option<u32>> = Mutex::new(None);
    static ARTWORK: Mutex<Option<(String, usize)>> = Mutex::new(None);
    struct OwnedImage(usize);
    impl OwnedImage {
        fn ptr(&self) -> id {
            self.0 as id
        }
    }
    impl Drop for OwnedImage {
        fn drop(&mut self) {
            unsafe {
                let _: () = msg_send![self.0 as id, release];
            }
        }
    }
    #[link(name = "MediaPlayer", kind = "framework")]
    extern "C" {
        static MPMediaItemPropertyTitle: id;
        static MPMediaItemPropertyArtist: id;
        static MPMediaItemPropertyAlbumTitle: id;
        static MPMediaItemPropertyPlaybackDuration: id;
        static MPMediaItemPropertyArtwork: id;
        static MPNowPlayingInfoPropertyElapsedPlaybackTime: id;
        static MPNowPlayingInfoPropertyPlaybackRate: id;
    }
    #[link(name = "IOKit", kind = "framework")]
    extern "C" {
        fn IOPMAssertionCreateWithName(kind: id, level: u32, name: id, assertion: *mut u32) -> i32;
        fn IOPMAssertionRelease(assertion: u32) -> i32;
    }
    unsafe fn string(value: &str) -> id {
        {
            let text = NSString::alloc(nil).init_str(value);
            msg_send![text, autorelease]
        }
    }
    unsafe fn command(command: id, kind: &'static str) {
        let handler = block::ConcreteBlock::new(move |event: id| -> isize {
            let mut data = serde_json::json!({"kind":kind});
            if kind == "seek" {
                let pos: f64 = msg_send![event, positionTime];
                data["position"] = serde_json::json!(pos);
            }
            if kind == "repeat" {
                let mode: isize = msg_send![event, repeatType];
                data["mode"] = serde_json::json!(mode);
            }
            if kind == "shuffle" {
                let mode: isize = msg_send![event, shuffleType];
                data["mode"] = serde_json::json!(mode);
            }
            if crate::events::emit("media.event", data).is_ok() {
                0
            } else {
                200
            }
        })
        .copy();
        let _: id = msg_send![command,addTargetWithHandler:&*handler];
        let _: () = msg_send![command,setEnabled:YES];
        // MPRemoteCommand copies and retains the block.
    }
    unsafe fn install() {
        INSTALL.call_once(|| {
            let center: id = msg_send![class!(MPRemoteCommandCenter), sharedCommandCenter];
            macro_rules! hook {
                ($sel:ident,$kind:literal) => {{
                    let c: id = msg_send![center, $sel];
                    command(c, $kind);
                }};
            }
            hook!(playCommand, "play");
            hook!(pauseCommand, "pause");
            hook!(togglePlayPauseCommand, "toggle");
            hook!(stopCommand, "stop");
            hook!(nextTrackCommand, "next");
            hook!(previousTrackCommand, "previous");
            hook!(changePlaybackPositionCommand, "seek");
            hook!(likeCommand, "favorite");
            hook!(changeRepeatModeCommand, "repeat");
            hook!(changeShuffleModeCommand, "shuffle");
        });
    }
    pub fn update(info: MediaInfo) -> bool {
        dispatch::Queue::main().exec_sync(move ||unsafe{
            install();
            let dict:id=msg_send![class!(NSMutableDictionary),dictionary];
            macro_rules! put {($key:ident,$value:expr)=>{{let value:id=$value;let _:()=msg_send![dict,setObject:value forKey:$key];}}}
            put!(MPMediaItemPropertyTitle,string(&info.title));put!(MPMediaItemPropertyArtist,string(&info.artist));put!(MPMediaItemPropertyAlbumTitle,string(&info.album));
            put!(MPMediaItemPropertyPlaybackDuration,msg_send![class!(NSNumber),numberWithDouble:info.duration.max(0.0)]);
            put!(MPNowPlayingInfoPropertyElapsedPlaybackTime,msg_send![class!(NSNumber),numberWithDouble:info.position.max(0.0)]);
            put!(MPNowPlayingInfoPropertyPlaybackRate,msg_send![class!(NSNumber),numberWithDouble:if info.playing {1.0f64}else{0.0f64}]);
            {
                let mut cache=ARTWORK.lock().unwrap();
                if cache.as_ref().map(|(p,_)|p)!=info.artwork_path.as_ref(){
                    if let Some((_,previous))=cache.take(){let _:()=msg_send![previous as id,release];}
                    if let Some(path)=info.artwork_path {
                        let image:id=msg_send![class!(NSImage),alloc];
                        let image:id=msg_send![image,initWithContentsOfFile:string(&path)];
                        if image!=nil {
                            let size:NSSize=msg_send![image,size];
                            let owned=std::sync::Arc::new(OwnedImage(image as usize));
                            let handler=block::ConcreteBlock::new(move |_size:NSSize|->id {owned.ptr()}).copy();
                            let artwork:id=msg_send![class!(MPMediaItemArtwork),alloc];
                            let artwork:id=msg_send![artwork,initWithBoundsSize:size requestHandler:&*handler];
                            if artwork!=nil{*cache=Some((path,artwork as usize));}
                        }
                    }
                }
                if let Some((_,artwork))=cache.as_ref(){put!(MPMediaItemPropertyArtwork,*artwork as id);}
            }
            let center:id=msg_send![class!(MPNowPlayingInfoCenter),defaultCenter];
            let _:()=msg_send![center,setNowPlayingInfo:dict];
            let _:()=msg_send![center,setPlaybackState:if info.playing{1usize}else{2usize}];
            let remote:id=msg_send![class!(MPRemoteCommandCenter),sharedCommandCenter];
            let next:id=msg_send![remote,nextTrackCommand];let prev:id=msg_send![remote,previousTrackCommand];let like:id=msg_send![remote,likeCommand];
            let _:()=msg_send![next,setEnabled:if info.can_next{YES}else{NO}];let _:()=msg_send![prev,setEnabled:if info.can_previous{YES}else{NO}];let _:()=msg_send![like,setActive:if info.favorite{YES}else{NO}];
            true
        })
    }
    pub fn clear() {
        dispatch::Queue::main().exec_sync(|| unsafe {
            let center: id = msg_send![class!(MPNowPlayingInfoCenter), defaultCenter];
            let _: () = msg_send![center,setNowPlayingInfo:nil];
            if let Some((_, artwork)) = ARTWORK.lock().unwrap().take() {
                let _: () = msg_send![artwork as id, release];
            }
            let _: () = msg_send![center,setPlaybackState:3usize];
        });
    }
    pub fn snapshot() -> serde_json::Value {
        dispatch::Queue::main().exec_sync(|| unsafe {
            let center: id = msg_send![class!(MPNowPlayingInfoCenter), defaultCenter];
            let info: id = msg_send![center, nowPlayingInfo];
            let state: usize = msg_send![center, playbackState];
            let title = if info != nil {
                let value: id = msg_send![info,objectForKey:MPMediaItemPropertyTitle];
                if value != nil {
                    let bytes: *const std::os::raw::c_char = msg_send![value, UTF8String];
                    std::ffi::CStr::from_ptr(bytes)
                        .to_string_lossy()
                        .into_owned()
                } else {
                    String::new()
                }
            } else {
                String::new()
            };
            let artwork:id=if info!=nil {msg_send![info,objectForKey:MPMediaItemPropertyArtwork]}else{nil};
            serde_json::json!({"title":title,"playback_state":state,"has_info":info!=nil,"has_artwork":artwork!=nil})
        })
    }
    pub fn keep_awake(enabled: bool) -> Result<bool, String> {
        let mut assertion = ASSERTION.lock().unwrap();
        if enabled && assertion.is_none() {
            let result = dispatch::Queue::main().exec_sync(|| unsafe {
                let mut value = 0;
                let status = IOPMAssertionCreateWithName(
                    string("PreventUserIdleDisplaySleep"),
                    255,
                    string("Haiku GPUIX now playing"),
                    &mut value,
                );
                (status, value)
            });
            if result.0 != 0 {
                return Err(format!("IOPM assertion failed: {}", result.0));
            }
            *assertion = Some(result.1);
        } else if !enabled {
            if let Some(value) = assertion.take() {
                unsafe {
                    IOPMAssertionRelease(value);
                }
            }
        }
        Ok(assertion.is_some())
    }
}

pub fn media_update(info: MediaInfo) -> bool {
    #[cfg(target_os = "macos")]
    {
        mac::update(info)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = info;
        false
    }
}
pub fn media_clear() {
    #[cfg(target_os = "macos")]
    mac::clear();
}
pub fn keep_awake(enabled: bool) -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    {
        mac::keep_awake(enabled)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = enabled;
        Err("Display sleep assertion is unsupported on this platform".into())
    }
}

pub fn media_snapshot() -> serde_json::Value {
    #[cfg(target_os = "macos")]
    {
        mac::snapshot()
    }
    #[cfg(not(target_os = "macos"))]
    {
        serde_json::json!({"supported":false})
    }
}
