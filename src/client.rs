extern crate futures;
extern crate grpcio;
#[macro_use]
extern crate log;

extern crate protos;

use std::sync::Arc;

use futures::{future, Future, Sink, Stream};
use grpcio::*;
use protos::service::{Command, Moment, CommandResult, Empty};
use protos::service_grpc::ExecutorClient;


fn main() {
    let env = Arc::new(Environment::new(2));
    let channel = ChannelBuilder::new(env).connect("127.0.0.1:9000");
    let client = ExecutorClient::new(channel);

    let empty = Empty::new();
    let mut subscribe = client.subscribe(&empty).unwrap();
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
    println!("Finish");
}
