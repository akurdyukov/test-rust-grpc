extern crate futures;
extern crate grpcio;
extern crate protos;

#[macro_use]
extern crate log;

use std::io::Read;
use std::sync::Arc;
use std::{io, thread};
use std::time::Duration;

use futures::*;
use futures::sync::oneshot;
use grpcio::*;
use grpcio::{Environment, RpcContext, ServerBuilder, UnarySink, ServerStreamingSink};

use protos::service::{Command, Moment, CommandResult, Empty};
use protos::service_grpc::{self, Executor};

#[derive(Clone)]
struct ExecutorService;

impl Executor for ExecutorService {
    fn execute(&self, ctx: RpcContext, command: Command, sink: UnarySink<Empty>) {
        println!("Received command {{ {:?} }}", command);
        let f = sink.success(Empty::new())
            .map(move |_| println!("Responded with empty"))
            .map_err(move |err| eprintln!("Failed to reply: {:?}", err));
        ctx.spawn(f)
    }

    fn subscribe(&self, ctx: RpcContext, command: Empty, resp: ServerStreamingSink<CommandResult>) {
        println!("Received subscribe {{ {:?} }}", command);

        let (mut sender, receiver) = sync::mpsc::channel::<(CommandResult, WriteFlags)>(256);
        let receiver2 = receiver.map_err::<Error, _>(|()| { Error::RemoteStopped });
        thread::spawn(move || {
            let mut i = 0;

            while i < 1000 {
                let mut result = CommandResult::new();
                result.command_id = i;

                sender.try_send((result.clone(), WriteFlags::default())).unwrap();
                i += 1;

                thread::sleep(Duration::from_secs(1));
            }
        });

        let f = resp
            .send_all(receiver2)
            .map(|_| { println!("Data sent")})
            .map_err(|e| error!("failed to handle subscribe request: {:?}", e));

        ctx.spawn(f)
    }
}

fn main() {
    let env = Arc::new(Environment::new(1));
    let service = service_grpc::create_executor(ExecutorService);
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
}
