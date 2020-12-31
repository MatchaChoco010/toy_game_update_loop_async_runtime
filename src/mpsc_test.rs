#![allow(dead_code)]

use std::sync::mpsc::sync_channel;
use std::thread;
use std::time::Duration;

pub fn main_mpsc_test() {
    let (sender, receiver) = sync_channel(0);

    // Nothing is in the buffer yet
    assert!(receiver.try_iter().next().is_none());
    println!("Nothing in the buffer...");

    thread::spawn(move || {
        thread::sleep(Duration::from_secs(2)); // block for two seconds
        sender.send(1).unwrap();
        sender.send(2).unwrap();
        sender.send(3).unwrap();
    });

    // for i in 1..=3 {
    //     sender.send(i).unwrap();
    // }

    println!("Going to sleep...");

    // for x in receiver.try_iter() {
    //     println!("Got: {}", x);
    // }

    // println!("{}", receiver.recv().unwrap());
    while let Ok(i) = receiver.recv() {
        println!("{}", i);
    }
}
