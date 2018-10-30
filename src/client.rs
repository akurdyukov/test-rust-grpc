extern crate futures;
extern crate grpcio;
#[macro_use]
extern crate log;
extern crate env_logger;

extern crate protos;

use std::io::Read;
use std::sync::Arc;
use std::{io, thread};

use futures::{future, Future, Sink, Stream};
use futures::sync::oneshot;
use grpcio::*;
use protos::service::{Command, Moment, CommandResult, Empty, SubscribeRequest};
use protos::service_grpc::ExecutorClient;

fn subscribe(client: &ExecutorClient) {
    let mut req = SubscribeRequest::new();
    req.user = String::from("user1");

    let mut subscribe = client.subscribe(&req).unwrap();
    thread::spawn(move || {
        loop {
            let f = subscribe.into_future();
            match f.wait() {
                Ok((Some(result), s)) => {
                    subscribe = s;
                    println!("Result: {:?}", result);
                },
                Ok((None, _)) => break,
                Err((e, _)) => panic!("Failed: {:?}", e)
            }
        }
    });
}

fn main() {
    let log_env = env_logger::Env::default()
        .filter_or(env_logger::DEFAULT_FILTER_ENV, "info");
 
    env_logger::Builder::from_env(log_env).init();

    let env = Arc::new(Environment::new(2));
    let channel = ChannelBuilder::new(env).connect("127.0.0.1:9000");
    let client = Box::new(ExecutorClient::new(channel));

    // subscribing thread
    subscribe(&client);

    // execution
    for i in 0..1000 {
        let mut command = Command::new();
        command.command_id = i;
        command.user = String::from("user1");

        client.execute(&command).unwrap();
    }

    let (tx, rx) = oneshot::channel();
    thread::spawn(move || {
        println!("Press ENTER to exit...");
        let _ = io::stdin().read(&mut [0]).unwrap();
        tx.send(())
    });
    let _ = rx.wait();

    println!("Finish");
}
