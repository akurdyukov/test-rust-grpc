extern crate futures;
extern crate grpcio;
extern crate protos;
extern crate tokio;

#[macro_use]
extern crate log;
extern crate env_logger;

extern crate multiqueue;

use std::io::Read;
use std::sync::{Arc, RwLock};
use std::{io, thread};
use std::time::Duration;
use std::collections::HashMap;

use futures::*;
use futures::sync::oneshot;
use grpcio::*;
use grpcio::{Environment, RpcContext, ServerBuilder, UnarySink, ServerStreamingSink};

use protos::service::{Command, Moment, CommandResult, Empty, SubscribeRequest};
use protos::service_grpc::{self, Executor};

struct Processor {
    user_id: String,
    count: u32,
    broadcaster_sender: sync::mpsc::Sender<CommandResult>,
}

impl Processor {
    fn new(user_id: String, broadcaster_sender: sync::mpsc::Sender<CommandResult>) -> Self {
        Processor {
            user_id,
            count: 0,
            broadcaster_sender
        }
    }

    fn do_process(&mut self, command: &Command) {
        info!("I'm processing {:?} for user {}: {}", command, self.user_id, self.count);
        self.count += 1;
        thread::sleep(Duration::from_millis(10));

        // send to broadcaster
        let mut result = CommandResult::new();
        result.command_id = command.command_id;
        result.user = command.user.clone();
        
        self.broadcaster_sender.clone().send_all(stream::once(Ok(result))).wait().ok();
    }
}

struct ProcessorMailbox {
    // user sends command here
    sender: sync::mpsc::Sender<Command>,
}

impl ProcessorMailbox {
    fn new(user: &String, broadcaster_sender: sync::mpsc::Sender<CommandResult>) -> Self {
        let (sender, receiver) = sync::mpsc::channel::<Command>(2);

        let result = ProcessorMailbox {
            sender,
        };

        let mut processor = Processor::new(user.clone(), broadcaster_sender);
        tokio::spawn(
            receiver
                .map(move |x| { processor.do_process(&x) })
                .fold((), |_, _| Ok(()))
        );

        result
    }

    fn send(&self, command: Command) {
        self.sender.clone().send_all(stream::once(Ok(command))).wait().ok();
    }
}


fn start_supervisor(broadcaster_sender: sync::mpsc::Sender<CommandResult>) -> sync::mpsc::Sender<Command> {
    let (sender, receiver) = sync::mpsc::channel::<Command>(2);
    let mut processors = HashMap::new();

    tokio::spawn(
        receiver
            .map(move |x| {
                info!("Got from receiver: {:?}", x);
                let user = x.user.clone();
                let user_to_create = x.user.clone();
                let acceptor = processors.entry(user)
                            .or_insert_with(|| ProcessorMailbox::new(&user_to_create, broadcaster_sender.clone()));
                acceptor.send(x);
            })
            .fold((), |_, _| Ok(()))
    );
    sender
}

struct Broadcaster {
    user_subscriptions: Arc<RwLock<HashMap<String, sync::mpsc::Sender<protos::service::CommandResult>>>>,
    sender: sync::mpsc::Sender<CommandResult>,
}

impl Broadcaster {
    fn new() -> Self {
        let (sender, receiver) = sync::mpsc::channel::<CommandResult>(2);
        let user_subscriptions = Arc::new(RwLock::new(HashMap::new()));

        let result = Broadcaster {
            sender,
            user_subscriptions: user_subscriptions.clone(),
        };

        let thread_map = user_subscriptions.clone();
        tokio::spawn(
            receiver
                .map(move |x| {
                    info!("Broadcasting command result: {:?}", x);
                    let user = x.user.clone();
                    let map = thread_map.read().unwrap();
                    if let Some(sink) = map.get(&user) {
                        sink.clone().send_all(stream::once(Ok(x))).wait().ok();
                    }
                })
                .fold((), |_, _| Ok(()))
        );

        result
    }

    fn subscribe(&self, user: String, sender: sync::mpsc::Sender<protos::service::CommandResult>) {
        let mut map = self.user_subscriptions.write().unwrap();

        match map.insert(user.clone(), sender) {
            None => info!("New subscription for {}", user.clone()),
            Some(_) => warn!("Subscription was here for {}", user.clone())
        }
    }

    fn unsubscribe(&self, user: String) {
        let mut map = self.user_subscriptions.write().unwrap();

        match map.remove(&user) {
            None => warn!("Nothing removed for {}", user.clone()),
            Some(_) => info!("Subscription was removed for {}", user.clone())
        }

    }

    fn get_sender(&self) -> sync::mpsc::Sender<CommandResult> {
        self.sender.clone()
    }
}


#[derive(Clone)]
struct ExecutorService {
    // send command to multiplexor
    multiplexor_sender: sync::mpsc::Sender<Command>,
    broadcaster: Arc<Broadcaster>,
}

impl ExecutorService {
    fn new(multiplexor_sender: sync::mpsc::Sender<Command>, broadcaster: Arc<Broadcaster>) -> Self {
        ExecutorService {
            multiplexor_sender,
            broadcaster,
        }
    }
}

impl Executor for ExecutorService {
    fn execute(&self, ctx: RpcContext, command: Command, sink: UnarySink<Empty>) {
        info!("Received command {{ {:?} }}", command);
        let multiplexor_sender = self.multiplexor_sender.clone();
        let f = sink.success(Empty::new())
            .map(move |_| {
                multiplexor_sender.clone().send_all(stream::once(Ok(command.clone()))).wait().ok();
                info!("Responded with empty")
            })
            .map_err(move |err| eprintln!("Failed to reply: {:?}", err));
        ctx.spawn(f)
    }

    fn subscribe(&self, ctx: RpcContext, req: SubscribeRequest, resp: ServerStreamingSink<CommandResult>) {
        info!("Received subscribe {{ {:?} }}", req);

        let (sender, receiver) = sync::mpsc::channel::<CommandResult>(256);
        self.broadcaster.subscribe(req.user.clone(), sender);

        let receiver2 = receiver
                .map(|v| {
                    info!("Sending {:?} to client", v.clone());
                    (v, WriteFlags::default())
                })
                .map_err::<Error, _>(|()| { Error::RemoteStopped });

        let user = req.user.clone();
        let broadcaster = self.broadcaster.clone();
        let f = resp
            .send_all(receiver2)
            .map(|_| { info!("Data sent")})
            .map_err(move |e| {
                match e {
                    Error::RemoteStopped => {
                        info!("Remote unsubscribed");
                        broadcaster.unsubscribe(user);
                    }
                    _ => {
                        error!("failed to handle subscribe request: {:?}", e);
                    }
                }
            });

        ctx.spawn(f)
    }
}

fn main() {
    let log_env = env_logger::Env::default()
        .filter_or(env_logger::DEFAULT_FILTER_ENV, "info");
 
    env_logger::Builder::from_env(log_env).init();

    let env = Arc::new(Environment::new(1));

    tokio::run(future::ok(()).map(|_| {
        let broadcaster = Arc::new(Broadcaster::new());

        let sender = start_supervisor(broadcaster.get_sender());

        let service = service_grpc::create_executor(ExecutorService::new(sender.clone(), broadcaster.clone()));
        
        let mut server = ServerBuilder::new(env)
            .register_service(service)
            .bind("127.0.0.1", 9000)
            .build()
            .unwrap();
        server.start();
        for &(ref host, port) in server.bind_addrs() {
            println!("listening on {}:{}", host, port);
        }

        let (tx, rx) = oneshot::channel();
        thread::spawn(move || {
            println!("Press ENTER to exit...");
            let _ = io::stdin().read(&mut [0]).unwrap();
            tx.send(())
        });
        let _ = rx.wait();
        let _ = server.shutdown().wait();
    }));
}
