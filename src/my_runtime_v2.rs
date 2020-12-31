#![allow(dead_code)]

use std::pin::Pin;
use std::task::Context;
use std::thread::sleep;
use std::time::{Duration, Instant};
use std::{future::Future, task::Poll};

use futures::{join, task};

enum MyRuntimeDone {
    NotDone,
    Done,
}

struct MyRuntime {
    current_index: u8,
    task_pool_0: Vec<Pin<Box<dyn Future<Output = ()> + 'static>>>,
    task_pool_1: Vec<Pin<Box<dyn Future<Output = ()> + 'static>>>,
}
impl MyRuntime {
    fn new() -> Self {
        Self {
            current_index: 0,
            task_pool_0: vec![],
            task_pool_1: vec![],
        }
    }

    fn spawn(&mut self, f: impl Future<Output = ()> + 'static) {
        self.task_pool_0.push(Box::pin(f));
    }

    fn update(&mut self) -> MyRuntimeDone {
        let waker = task::noop_waker();
        let mut cx = Context::from_waker(&waker);

        if self.current_index == 0 {
            while let Some(mut task) = self.task_pool_0.pop() {
                if task.as_mut().poll(&mut cx).is_pending() {
                    self.task_pool_1.push(task);
                }
            }
            self.current_index = 1;
            if self.task_pool_1.len() == 0 {
                return MyRuntimeDone::Done;
            } else {
                return MyRuntimeDone::NotDone;
            }
        } else {
            while let Some(mut task) = self.task_pool_1.pop() {
                if task.as_mut().poll(&mut cx).is_pending() {
                    self.task_pool_0.push(task);
                }
            }
            self.current_index = 0;
            if self.task_pool_0.len() == 0 {
                return MyRuntimeDone::Done;
            } else {
                return MyRuntimeDone::NotDone;
            }
        }
    }
}

struct WaitNextFrame {
    polled: bool,
}
impl WaitNextFrame {
    fn new() -> Self {
        Self { polled: false }
    }
}
impl Future for WaitNextFrame {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<()> {
        if self.polled {
            Poll::Ready(())
        } else {
            self.polled = true;
            Poll::Pending
        }
    }
}

async fn ten_frame_task(id: u8) {
    for i in 0..10 {
        println!("TaskID: {}, Frame: {}", id, i);
        WaitNextFrame::new().await;
    }
}

pub fn main_v2() {
    let mut rt = MyRuntime::new();

    rt.spawn(async {
        ten_frame_task(0).await;
        ten_frame_task(1).await;
        let t2 = ten_frame_task(2);
        let t3 = ten_frame_task(3);
        join!(t2, t3);
        ten_frame_task(4).await;
    });

    // update loop
    'update_loop: loop {
        let frame_start = Instant::now();

        match rt.update() {
            MyRuntimeDone::NotDone => (),
            MyRuntimeDone::Done => break 'update_loop,
        }

        let now = Instant::now();
        let duration = now - frame_start;
        if duration < Duration::new(0, 16666666) {
            sleep(Duration::new(0, 16666666) - duration);
        }
    }
}
