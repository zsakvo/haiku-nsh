//! Isolated plugin runtime using the same engine as NSH.
//!
//! Each instance owns one OS thread and one QuickJS Runtime/Context. Commands
//! execute serially; only strings, JSON and errors cross the channel. A local
//! Tokio executor and wake-driven job polling run JS promises and stdlib futures.
//! Generic host facilities; no music SDK semantics. This is not an OS sandbox.

use std::{
    collections::BTreeMap,
    fmt,
    future::{Future, poll_fn},
    path::{Path, PathBuf},
    pin::pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    task::{Context as TaskContext, Poll},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use rquickjs::{
    AsyncContext, AsyncRuntime, Ctx, Function, Module, Object, Persistent, Value as JsValue,
    promise::MaybePromise,
};
use serde_json::Value;
use tokio::sync::{Notify, mpsc as channel, oneshot};

mod crypto;
mod host;
mod loader;
pub use host::{HttpEvent, LogRecord};
pub use loader::NativeModule;

const LOAD_TIMEOUT: Duration = Duration::from_secs(5);
const JOB_BATCH: usize = 32;

/// A JavaScript exception or a named host lifecycle error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsError {
    /// JavaScript exception name, or `TimeoutError`, `RebuildRequired`,
    /// `ShutdownError`, `RuntimeError`, or `InvalidArgument`.
    pub name: String,
    /// Human-readable reason.
    pub message: String,
    /// JavaScript stack when supplied by the thrown value; otherwise empty.
    pub stack: String,
    /// Original source location, when available (one-based).
    pub file: Option<String>,
    pub line: Option<u32>,
    pub column: Option<u32>,
    /// Enumerable exception fields (host-specific codes, retry metadata).
    pub details: Option<Value>,
}

impl JsError {
    fn new(name: &str, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            message: message.into(),
            stack: String::new(),
            file: None,
            line: None,
            column: None,
            details: None,
        }
    }

    fn from_engine(ctx: &Ctx<'_>, error: rquickjs::Error) -> Self {
        if !matches!(error, rquickjs::Error::Exception) {
            return Self::new("RuntimeError", error.to_string());
        }
        let thrown = ctx.catch();
        if let Some(object) = thrown.as_object() {
            Self {
                name: object.get("name").unwrap_or_else(|_| "Error".into()),
                message: object
                    .get("message")
                    .unwrap_or_else(|_| format!("{thrown:?}")),
                stack: object.get("stack").unwrap_or_default(),
                file: object.get("fileName").ok(),
                line: object.get("lineNumber").ok(),
                column: object.get("columnNumber").ok(),
                details: ctx
                    .json_stringify(thrown.clone())
                    .ok()
                    .flatten()
                    .and_then(|s| s.to_string().ok())
                    .and_then(|s| serde_json::from_str(&s).ok()),
            }
        } else {
            let message = rquickjs::Coerced::<String>::from_js(ctx, thrown)
                .map(|value| value.0)
                .unwrap_or_else(|_| "Non-Error JavaScript exception".into());
            Self::new("Error", message)
        }
    }
}

use rquickjs::FromJs;

impl fmt::Display for JsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.name, self.message)
    }
}

impl std::error::Error for JsError {}

/// Configuration retained across unload and hot reload. Callbacks run on the VM thread.
#[derive(Clone)]
pub struct RuntimeOptions {
    pub instance_id: String,
    pub memory_limit: usize,
    pub describe_only: bool,
    pub storage_path: Option<PathBuf>,
    pub modules: BTreeMap<String, String>,
    pub native_modules: BTreeMap<String, NativeModule>,
    pub log: Option<Arc<dyn Fn(LogRecord) + Send + Sync>>,
    pub http_hook: Option<Arc<dyn Fn(HttpEvent) + Send + Sync>>,
}
impl Default for RuntimeOptions {
    fn default() -> Self {
        Self {
            instance_id: "plugin".into(),
            memory_limit: 64 * 1024 * 1024,
            describe_only: false,
            storage_path: None,
            modules: BTreeMap::new(),
            native_modules: BTreeMap::new(),
            log: None,
            http_hook: None,
        }
    }
}

/// Cloneable cancellation token. Aborting does not destroy the instance.
#[derive(Clone, Default)]
pub struct AbortHandle(Arc<Stop>);
impl AbortHandle {
    pub fn abort(&self) {
        self.0.requested.store(true, Ordering::Release);
        self.0.wake.notify_waiters();
    }
    pub fn is_aborted(&self) -> bool {
        self.0.requested.load(Ordering::Acquire)
    }
    async fn cancelled(&self) {
        loop {
            let notified = self.0.wake.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.is_aborted() {
                return;
            }
            notified.await;
        }
    }
}
/// An enqueued call and its independently cloneable abort handle.
pub struct CallHandle {
    pub abort: AbortHandle,
    response: oneshot::Receiver<Result<Value, JsError>>,
}
impl CallHandle {
    pub async fn result(self) -> Result<Value, JsError> {
        self.response
            .await
            .map_err(|_| JsError::new("ShutdownError", "VM thread is stopped"))?
    }
}

enum Operation {
    Load {
        name: String,
        source: String,
    },
    Call {
        export: String,
        args: String,
        context: String,
    },
    Package {
        root: PathBuf,
        entry: String,
    },
    Unload,
    Describe,
}

struct Command {
    operation: Operation,
    deadline: Instant,
    abort: AbortHandle,
    reply: oneshot::Sender<Result<Value, JsError>>,
}

#[derive(Default)]
struct Stop {
    requested: AtomicBool,
    wake: Notify,
}

/// A channel handle to a dedicated plugin VM thread.
///
/// Calls on the same handle are serialized. Different instances run in parallel.
/// Dropping the handle interrupts JS, cancels pending work, and joins its thread.
/// JS handles never leave that thread, even if Cargo enables `parallel` features.
pub struct Runtime {
    commands: channel::UnboundedSender<Command>,
    stop: Arc<Stop>,
    thread: Option<JoinHandle<()>>,
}

impl Runtime {
    /// Starts a VM and installs the stdlib globals and builtin module loader.
    ///
    /// Returns only after initialization succeeds. Does not require an ambient
    /// Tokio runtime. Uses the default memory limit and in-memory storage.
    pub fn spawn() -> Result<Self, JsError> {
        Self::with_options(RuntimeOptions::default())
    }

    pub fn with_options(options: RuntimeOptions) -> Result<Self, JsError> {
        let (commands, receiver) = channel::unbounded_channel();
        let (startup, ready) = mpsc::sync_channel(1);
        let stop = Arc::new(Stop::default());
        let worker_stop = stop.clone();
        let thread = thread::Builder::new()
            .name("haiku-jsrt".into())
            .stack_size(4 * 1024 * 1024)
            .spawn(move || {
                let executor = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(executor) => executor,
                    Err(error) => {
                        let _ = startup.send(Err(JsError::new("RuntimeError", error.to_string())));
                        return;
                    }
                };
                executor.block_on(async move {
                    match Engine::configured(worker_stop.clone(), options.clone()).await {
                        Ok(engine) => {
                            if startup.send(Ok(())).is_ok() {
                                worker(engine, receiver, worker_stop, options).await;
                            }
                        }
                        Err(error) => {
                            let _ = startup.send(Err(error));
                        }
                    }
                });
            })
            .map_err(|error| JsError::new("RuntimeError", error.to_string()))?;
        match ready.recv() {
            Ok(Ok(())) => Ok(Self {
                commands,
                stop,
                thread: Some(thread),
            }),
            result => {
                let _ = thread.join();
                Err(result
                    .unwrap_or_else(|_| {
                        Err(JsError::new(
                            "RuntimeError",
                            "VM thread exited during startup",
                        ))
                    })
                    .unwrap_err())
            }
        }
    }

    /// Evaluates an ES module from JS source and selects its exports for `call`.
    ///
    /// Each name must be unique within the instance. Registered host modules and
    /// stdlib modules are available; use load_package for filesystem TypeScript.
    /// Top-level await is supported, with a fixed five-second load deadline.
    /// A timeout retires the VM; construct a new instance before using it again.
    pub async fn load_module(&self, name: &str, source: &str) -> Result<(), JsError> {
        self.request(
            Operation::Load {
                name: name.into(),
                source: source.into(),
            },
            LOAD_TIMEOUT,
        )
        .await
        .map(|_| ())
    }

    /// Calls a function exported by the last successfully loaded module.
    ///
    /// `args_json` is one JSON argument (use a JSON array for an array argument).
    /// The sync value or resolved Promise must be JSON serializable. `timeout`
    /// starts when the command is enqueued, including queue time. Dropping this
    /// future does not cancel the command; the deadline still applies.
    ///
    /// `TimeoutError` retires the VM, and later requests return `RebuildRequired`.
    /// Interrupts stop interpreted JS, including dead loops; they cannot preempt
    /// a synchronous native stdlib call. Its deadline is checked on return.
    pub async fn call(
        &self,
        export: &str,
        args_json: &str,
        timeout: Duration,
    ) -> Result<Value, JsError> {
        self.request(
            Operation::Call {
                export: export.into(),
                args: args_json.into(),
                context: "{}".into(),
            },
            timeout,
        )
        .await
    }

    /// Enqueue a call; args is the first argument and a fresh ctx is the second.
    /// Host JSON fields are augmented with signal/http/storage/log facilities.
    pub fn start_call(
        &self,
        export: &str,
        args: Value,
        context: Value,
        timeout: Duration,
    ) -> Result<CallHandle, JsError> {
        if !context.is_object() {
            return Err(JsError::new("InvalidArgument", "ctx must be an object"));
        }
        self.enqueue(
            Operation::Call {
                export: export.into(),
                args: args.to_string(),
                context: context.to_string(),
            },
            timeout,
        )
    }
    /// Loads a package into a fresh VM. Entry is relative to the canonical package root.
    pub async fn load_package(&self, root: &Path, entry: &str) -> Result<(), JsError> {
        self.request(
            Operation::Package {
                root: root.into(),
                entry: entry.into(),
            },
            LOAD_TIMEOUT,
        )
        .await
        .map(|_| ())
    }
    /// Queue a fresh package evaluation after earlier calls complete; storage survives.
    pub async fn reload(&self, root: &Path, entry: &str) -> Result<(), JsError> {
        self.load_package(root, entry).await
    }
    pub async fn unload(&self) -> Result<(), JsError> {
        self.request(Operation::Unload, LOAD_TIMEOUT)
            .await
            .map(|_| ())
    }
    /// Read an export shape without invoking getters or handler functions.
    pub async fn describe(&self) -> Result<Value, JsError> {
        self.request(Operation::Describe, LOAD_TIMEOUT).await
    }

    fn enqueue(&self, operation: Operation, timeout: Duration) -> Result<CallHandle, JsError> {
        let deadline = Instant::now()
            .checked_add(timeout)
            .ok_or_else(|| JsError::new("InvalidArgument", "Timeout is too large"))?;
        let (reply, response) = oneshot::channel();
        let abort = AbortHandle::default();
        self.commands
            .send(Command {
                operation,
                deadline,
                reply,
                abort: abort.clone(),
            })
            .map_err(|_| JsError::new("ShutdownError", "VM thread is stopped"))?;
        Ok(CallHandle { abort, response })
    }
    async fn request(&self, operation: Operation, timeout: Duration) -> Result<Value, JsError> {
        self.enqueue(operation, timeout)?.result().await
    }

    /// Interrupts execution, drops all VM state on its thread, and joins it.
    ///
    /// Also performed on Drop. Pending commands receive `ShutdownError`.
    /// This is a blocking join; callers on a UI thread should move ownership to
    /// their background executor before shutting down.
    pub fn shutdown(mut self) -> Result<(), JsError> {
        self.join()
    }

    fn join(&mut self) -> Result<(), JsError> {
        self.stop.requested.store(true, Ordering::Release);
        self.stop.wake.notify_one();
        if let Some(thread) = self.thread.take() {
            thread
                .join()
                .map_err(|_| JsError::new("RuntimeError", "VM thread panicked"))?;
        }
        Ok(())
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        let _ = self.join();
    }
}

// Field order ensures JS persistent values are released before context/runtime.
struct Engine {
    module: Option<Persistent<Object<'static>>>,
    controller: Option<Persistent<Object<'static>>>,
    options: RuntimeOptions,
    sources: loader::SharedSources,
    active: Arc<Mutex<AbortHandle>>,
    context: AsyncContext,
    runtime: AsyncRuntime,
    deadline: Arc<Mutex<Instant>>,
}

impl Engine {
    #[cfg(test)]
    async fn new(stop: Arc<Stop>) -> Result<Self, JsError> {
        Self::configured(stop, RuntimeOptions::default()).await
    }

    async fn configured(stop: Arc<Stop>, options: RuntimeOptions) -> Result<Self, JsError> {
        let runtime =
            AsyncRuntime::new().map_err(|error| JsError::new("RuntimeError", error.to_string()))?;
        let deadline = Arc::new(Mutex::new(Instant::now() + LOAD_TIMEOUT));
        let limit = deadline.clone();
        let active = Arc::new(Mutex::new(AbortHandle::default()));
        let interrupt = active.clone();
        runtime
            .set_interrupt_handler(Some(Box::new(move || {
                stop.requested.load(Ordering::Acquire)
                    || interrupt.lock().unwrap().is_aborted()
                    || Instant::now() >= *limit.lock().unwrap()
            })))
            .await;
        let sources = loader::SharedSources::default();
        let names = options
            .modules
            .keys()
            .chain(options.native_modules.keys())
            .cloned()
            .collect();
        let mut stdlib = quickjs_jit_stdlib::loader();
        stdlib.add_module("__stdlib_crypto", quickjs_jit_stdlib::crypto::CryptoModule);
        let mut modules = options.modules.clone();
        modules.insert("crypto".into(), crypto::MODULE.into());
        runtime
            .set_loader(
                (
                    quickjs_jit_stdlib::resolver().with_module("__stdlib_crypto"),
                    loader::PackageResolver {
                        sources: sources.clone(),
                        names,
                    },
                ),
                (
                    loader::PackageLoader {
                        sources: sources.clone(),
                        modules,
                        native: options.native_modules.clone(),
                    },
                    stdlib,
                ),
            )
            .await;
        runtime.set_memory_limit(options.memory_limit).await;
        runtime.set_max_stack_size(1024 * 1024).await;
        let context = AsyncContext::full(&runtime)
            .await
            .map_err(|error| JsError::new("RuntimeError", error.to_string()))?;
        context
            .with(|ctx| {
                (|| {
                    quickjs_jit_stdlib::init(&ctx)?;
                    quickjs_jit_stdlib::events::init(&ctx)?;
                    // The stdlib installs a DOMException stack accessor. Restore
                    // QuickJS's lazy Error stack accessor after that initialization.
                    let restore_stack: Function = ctx.eval(r#"(() => {
                        const descriptor=Object.getOwnPropertyDescriptor(Error.prototype,'stack');
                        return () => { if(descriptor) Object.defineProperty(Error.prototype,'stack',descriptor);
                            else delete Error.prototype.stack; };
                    })()"#)?;
                    quickjs_jit_stdlib::exceptions::init(&ctx)?;
                    restore_stack.call::<_, ()>(())?;
                    quickjs_jit_stdlib::abort::init(&ctx)?;
                    host::install(&ctx, &options)?;
                    crypto::install(&ctx)?;
                    Ok(())
                })()
                .map_err(|error| JsError::from_engine(&ctx, error))
            })
            .await?;
        Ok(Self {
            module: None,
            controller: None,
            options,
            sources,
            active,
            context,
            runtime,
            deadline,
        })
    }

    async fn execute(&mut self, operation: Operation) -> Result<Value, JsError> {
        match operation {
            Operation::Load { name, source } => {
                let (namespace, promise) = self
                    .context
                    .with(|ctx| {
                        let result = (|| {
                            let (module, promise) =
                                Module::declare(ctx.clone(), name, source)?.eval()?;
                            Ok((
                                Persistent::save(&ctx, module.namespace()?),
                                Persistent::save(&ctx, promise.into_value()),
                            ))
                        })();
                        result.map_err(|error| JsError::from_engine(&ctx, error))
                    })
                    .await?;
                self.settle(promise, false).await?;
                self.module = Some(namespace);
                Ok(Value::Null)
            }
            Operation::Package { root, entry } => {
                let root = root
                    .canonicalize()
                    .map_err(|e| JsError::new("ModuleError", e.to_string()))?;
                let path = loader::checked_path(&root, &root, &entry)?;
                let source = {
                    let mut sources = self.sources.lock().unwrap();
                    sources.root = Some(root);
                    loader::read_source(&path, &mut sources)?
                };
                Box::pin(self.execute(Operation::Load {
                    name: path.to_string_lossy().into_owned(),
                    source,
                }))
                .await
            }
            Operation::Unload => Ok(Value::Null),
            Operation::Describe => {
                let module = self
                    .module
                    .as_ref()
                    .ok_or_else(|| JsError::new("RuntimeError", "No module loaded"))?;
                self.context
                    .with(|ctx| {
                        let describe: Function = ctx
                            .globals()
                            .get("__describe")
                            .map_err(|e| JsError::from_engine(&ctx, e))?;
                        let text: String = describe
                            .call((module
                                .clone()
                                .restore(&ctx)
                                .map_err(|e| JsError::from_engine(&ctx, e))?,))
                            .map_err(|e| JsError::from_engine(&ctx, e))?;
                        serde_json::from_str(&text)
                            .map_err(|e| JsError::new("RuntimeError", e.to_string()))
                    })
                    .await
            }
            Operation::Call {
                export,
                args,
                context,
            } => {
                if self.options.describe_only {
                    return Err(JsError::new("DescribeOnly", "Handler calls are disabled"));
                }
                let module = self.module.as_ref().ok_or_else(|| {
                    JsError::new("RuntimeError", "Load a module before calling an export")
                })?;
                let value = self
                    .context
                    .with(|ctx| {
                        let result = (|| {
                            let namespace = module.clone().restore(&ctx)?;
                            let mut object = namespace;
                            let mut parts = export.split('.').peekable();
                            let mut key = parts.next().unwrap_or("");
                            while parts.peek().is_some() {
                                object = object.get(key)?;
                                key = parts.next().unwrap();
                            }
                            let function: Function = object.get(key)?;
                            let make: Function = ctx.globals().get("__makeContext")?;
                            let pair: Object = make.call((ctx.json_parse(context)?,))?;
                            self.controller =
                                Some(Persistent::save(&ctx, pair.get::<_, Object>("controller")?));
                            let call_ctx: Object = pair.get("ctx")?;
                            let args = ctx.json_parse(args)?;
                            let value: JsValue = function.call((
                                rquickjs::function::This(object),
                                args,
                                call_ctx,
                            ))?;
                            Ok(Persistent::save(&ctx, value))
                        })();
                        result.map_err(|error| JsError::from_engine(&ctx, error))
                    })
                    .await?;
                self.settle(value, true).await
            }
        }
    }

    async fn settle(
        &self,
        value: Persistent<JsValue<'static>>,
        json: bool,
    ) -> Result<Value, JsError> {
        poll_fn(|task| {
            for _ in 0..JOB_BATCH {
                let result = ready(
                    self.context.with(|ctx| {
                        let result = (|| {
                            let promise = MaybePromise::from_value(value.clone().restore(&ctx)?);
                            promise
                                .result::<JsValue>()
                                .map(|result| {
                                    result.and_then(|value| {
                                        if !json {
                                            return Ok(None);
                                        }
                                        ctx.json_stringify(value)?
                                            .ok_or_else(|| {
                                                rquickjs::Error::new_into_js("undefined", "JSON")
                                            })?
                                            .to_string()
                                            .map(Some)
                                    })
                                })
                                .transpose()
                        })();
                        result.map_err(|error| JsError::from_engine(&ctx, error))
                    }),
                    task,
                );
                match result {
                    Ok(Some(text)) => {
                        return Poll::Ready(match text {
                            Some(text) => serde_json::from_str(&text)
                                .map_err(|error| JsError::new("RuntimeError", error.to_string())),
                            None => Ok(Value::Null),
                        });
                    }
                    Err(error) => return Poll::Ready(Err(error)),
                    Ok(None) => {}
                }
                match self.poll_job(task) {
                    Poll::Ready(Ok(true)) => {}
                    Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                    Poll::Pending | Poll::Ready(Ok(false)) => return Poll::Pending,
                }
            }
            // Yield between finite batches so deadlines and shutdown can run.
            task.waker().wake_by_ref();
            Poll::Pending
        })
        .await
    }

    fn poll_job(&self, task: &mut TaskContext<'_>) -> Poll<Result<bool, JsError>> {
        let future = pin!(self.runtime.execute_pending_job());
        future.poll(task).map(|result| {
            result.map_err(|error| {
                ready(
                    error
                        .0
                        .with(|ctx| JsError::from_engine(&ctx, rquickjs::Error::Exception)),
                    task,
                )
            })
        })
    }

    async fn idle(&self) -> Result<(), JsError> {
        poll_fn(|task| {
            for _ in 0..JOB_BATCH {
                match self.poll_job(task) {
                    Poll::Ready(Ok(true)) => {}
                    Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                    Poll::Pending | Poll::Ready(Ok(false)) => return Poll::Pending,
                }
            }
            task.waker().wake_by_ref();
            Poll::Pending
        })
        .await
    }
}

// Like nsh-js's scheduler: all context entry stays local and non-nested. Only job
// polling and Tokio I/O may suspend; a contended context lock is a host bug.
fn ready<F: Future>(future: F, task: &mut TaskContext<'_>) -> F::Output {
    match pin!(future).poll(task) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("nested haiku-jsrt context entry"),
    }
}

async fn worker(
    engine: Engine,
    mut commands: channel::UnboundedReceiver<Command>,
    stop: Arc<Stop>,
    options: RuntimeOptions,
) {
    let mut engine = Some(engine);
    loop {
        if stop.requested.load(Ordering::Acquire) {
            break;
        }
        let command = if let Some(vm) = engine.as_ref() {
            tokio::select! {
                biased;
                _ = stop.wake.notified() => break,
                command = commands.recv() => command,
                _ = vm.idle() => {
                    // An uncaught detached job error also invalidates this VM.
                    engine.take();
                    continue;
                }
            }
        } else {
            tokio::select! {
                _ = stop.wake.notified() => break,
                command = commands.recv() => command,
            }
        };
        let Some(command) = command else {
            break;
        };
        if matches!(
            command.operation,
            Operation::Package { .. } | Operation::Unload
        ) {
            engine.take();
            match Engine::configured(stop.clone(), options.clone()).await {
                Ok(vm) => engine = Some(vm),
                Err(error) => {
                    let _ = command.reply.send(Err(error));
                    continue;
                }
            }
        }
        let result = if let Some(vm) = engine.as_mut() {
            *vm.active.lock().unwrap() = command.abort.clone();
            *vm.deadline.lock().unwrap() = command.deadline;
            let result = if command.abort.is_aborted() {
                Err(JsError::new("AbortError", "Call aborted"))
            } else if Instant::now() >= command.deadline {
                Err(timeout_error())
            } else {
                tokio::select! {
                    biased;
                    _ = stop.wake.notified() => Err(JsError::new("ShutdownError", "VM is shutting down")),
                    _ = command.abort.cancelled() => Err(JsError::new("AbortError", "Call aborted")),
                    result = tokio::time::timeout_at(
                        command.deadline.into(), vm.execute(command.operation)
                    ) => result.unwrap_or_else(|_| Err(timeout_error())),
                }
            };
            *vm.active.lock().unwrap() = AbortHandle::default();
            if command.abort.is_aborted() {
                if let Some(controller) = vm.controller.take() {
                    vm.context
                        .with(|ctx| {
                            if let Ok(controller) = controller.restore(&ctx)
                                && let Ok(abort) = controller.get::<_, Function>("abort")
                            {
                                let _: rquickjs::Result<()> =
                                    abort.call((rquickjs::function::This(controller),));
                            }
                        })
                        .await;
                }
                Err(JsError::new("AbortError", "Call aborted"))
            } else if stop.requested.load(Ordering::Acquire) {
                Err(JsError::new("ShutdownError", "VM is shutting down"))
            } else if Instant::now() >= command.deadline {
                Err(timeout_error())
            } else {
                result
            }
        } else {
            Err(JsError::new(
                "RebuildRequired",
                "VM was retired; spawn a new Runtime",
            ))
        };
        if result.as_ref().is_err_and(|error| {
            error.name == "TimeoutError" && error.message.starts_with("JS deadline exceeded")
        }) {
            engine.take(); // Cancel all remaining futures/jobs and release the VM.
        }
        let result = result.map_err(|mut e| {
            if let Some(vm) = engine.as_ref() {
                loader::locate(&mut e, &vm.sources);
            }
            if e.message.contains("out of memory") {
                e.name = "MemoryLimitError".into();
            }
            e
        });
        if result.as_ref().is_err_and(|e| e.name == "MemoryLimitError") {
            engine.take();
        }
        let _ = command.reply.send(result);
    }
    drop(engine);
    commands.close();
    while let Some(command) = commands.recv().await {
        let _ = command
            .reply
            .send(Err(JsError::new("ShutdownError", "VM is stopped")));
    }
}

fn timeout_error() -> JsError {
    JsError::new(
        "TimeoutError",
        "JS deadline exceeded; VM retired, spawn a new Runtime",
    )
}

#[cfg(test)]
mod tests;
