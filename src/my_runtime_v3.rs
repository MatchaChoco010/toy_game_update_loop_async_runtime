#![allow(dead_code)]

use futures::task::ArcWake;
use std::cell::RefCell;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::task::Context;
use std::task::Waker;
use std::thread::sleep;
use std::time::{Duration, Instant};
use std::{future::Future, task::Poll};

use futures::join;

struct Task {
    future: Pin<Box<dyn Future<Output = ()> + 'static>>,
}
impl Task {
    fn new(f: impl Future<Output = ()> + 'static) -> Self {
        Self {
            future: Box::pin(f),
        }
    }

    fn poll(&mut self, mut ctx: Context) -> Poll<()> {
        match Future::poll(self.future.as_mut(), &mut ctx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(()) => Poll::Ready(()),
        }
    }
}

#[derive(Clone)]
struct WakeFlag {
    waked: Arc<Mutex<bool>>,
}
impl WakeFlag {
    fn new() -> Self {
        Self {
            waked: Arc::new(Mutex::new(false)),
        }
    }

    fn wake(&self) {
        *self.waked.lock().unwrap() = true;
    }

    fn is_waked(&self) -> bool {
        *self.waked.lock().unwrap()
    }
}

#[derive(Clone)]
struct WakeFlagWaker {
    flag: WakeFlag,
}
impl WakeFlagWaker {
    fn waker(flag: WakeFlag) -> Waker {
        futures::task::waker(Arc::new(Self { flag }))
    }
}
impl ArcWake for WakeFlagWaker {
    fn wake_by_ref(arc_self: &Arc<Self>) {
        arc_self.flag.wake();
    }
}

#[derive(Clone)]
struct Runtime {
    frame_counter: usize,
    tasks_queue: Rc<RefCell<Vec<Task>>>,
    wait_tasks: Rc<RefCell<Vec<Task>>>,
}
impl Runtime {
    fn new() -> Self {
        Self {
            frame_counter: 0,
            tasks_queue: Rc::new(RefCell::new(vec![])),
            wait_tasks: Rc::new(RefCell::new(vec![])),
        }
    }

    fn spawn(&self, f: impl Future<Output = ()> + 'static) {
        self.tasks_queue.borrow_mut().push(Task::new(f));
    }

    fn run(&mut self, f: impl Future<Output = ()> + 'static) {
        self.frame_counter = 0;
        self.spawn(f);

        'update_loop: loop {
            let frame_start = Instant::now();

            println!("Frame count: {}", self.frame_counter);

            'current_frame: loop {
                let task = self.tasks_queue.borrow_mut().pop();

                match task {
                    None => break 'current_frame,
                    Some(mut task) => {
                        let flag = WakeFlag::new();
                        let waker = WakeFlagWaker::waker(flag.clone());

                        match task.poll(Context::from_waker(&waker)) {
                            Poll::Ready(()) => (),
                            Poll::Pending => {
                                // タスクがwake済みだったらtask_queueにpush
                                // そうでなかったらwait_tasksにpushする
                                if flag.is_waked() {
                                    self.tasks_queue.borrow_mut().push(task);
                                } else {
                                    self.wait_tasks.borrow_mut().push(task);
                                }
                            }
                        }
                    }
                }
            }

            // wait_tasksが空の場合、全てのタスクの実行が終わっている。
            if self.wait_tasks.borrow().is_empty() {
                break 'update_loop;
            }

            // 1/60待つ
            let now = Instant::now();
            let duration = now - frame_start;
            if duration < Duration::new(0, 16666666) {
                sleep(Duration::new(0, 16666666) - duration);
            }

            // 次のフレームに移る前にフレームカウンターを更新する
            self.frame_counter += 1;

            // wait_tasksを空のtasks_queueと交換する
            std::mem::swap(&mut self.wait_tasks, &mut self.tasks_queue);
        }
    }
}

struct WaitNextFrameFuture {
    polled: bool,
}
impl WaitNextFrameFuture {
    fn new() -> Self {
        Self { polled: false }
    }
}
impl Future for WaitNextFrameFuture {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.polled {
            Poll::Ready(())
        } else {
            self.polled = true;
            Poll::Pending
        }
    }
}

fn wait_next_frame() -> WaitNextFrameFuture {
    WaitNextFrameFuture::new()
}

async fn ten_frame_task(id: u8) {
    for i in 0..9 {
        println!("TaskID: {}, Frame: {}", id, i);
        wait_next_frame().await;
    }
    println!("TaskID: {}, Frame: {}", id, 9);
}

pub fn main_v3() {
    let mut runtime = Runtime::new();
    let r = runtime.clone();

    println!("---- First test ----");

    runtime.run(async move {
        r.spawn(async {
            ten_frame_task(5).await;
        });

        ten_frame_task(0).await;
        wait_next_frame().await;

        ten_frame_task(1).await;
        wait_next_frame().await;

        let t2 = ten_frame_task(2);
        let t3 = ten_frame_task(3);
        join!(t2, t3);
        wait_next_frame().await;

        ten_frame_task(4).await;
    });

    assert_eq!(runtime.frame_counter + 1, 40);

    println!("---- Second test ----");

    runtime.run(async {
        async {
            println!("Hi!");
        }
        .await;
        async {
            println!("Hoge");
        }
        .await;
        wait_next_frame().await;
        async {
            println!("Fuga");
        }
        .await;
    });

    assert_eq!(runtime.frame_counter, 1);
}
