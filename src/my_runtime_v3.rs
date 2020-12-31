#![allow(dead_code)]

use futures::task::ArcWake;
use std::collections::BTreeMap;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::Context;
use std::task::Waker;
use std::thread::sleep;
use std::time::{Duration, Instant};
use std::{future::Future, task::Poll};

use futures::{
    channel::mpsc::{channel, Receiver, Sender},
    join,
};

struct Task {
    future: Pin<Box<dyn Future<Output = ()> + 'static>>,
    waked: bool,
}
impl Task {
    fn new(f: impl Future<Output = ()> + 'static) -> Self {
        Self {
            future: Box::pin(f),
            waked: false,
        }
    }

    fn poll(&mut self, mut ctx: Context) -> Poll<()> {
        match Future::poll(self.future.as_mut(), &mut ctx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(()) => Poll::Ready(()),
        }
    }

    fn wake(&mut self) {
        self.waked = true;
    }
}

type TaskId = usize;
struct Runtime {
    frame_counter: usize,
    task_id_counter: TaskId,
    tasks_queue: Vec<Task>,
    wait_tasks: BTreeMap<TaskId, Task>,
    receiver: Receiver<TaskId>,
    sender: Sender<TaskId>,
}
impl Runtime {
    fn new() -> Self {
        let (sender, receiver) = channel(10);

        Self {
            frame_counter: 0,
            task_id_counter: 0,
            tasks_queue: vec![],
            wait_tasks: BTreeMap::new(),
            receiver,
            sender,
        }
    }

    fn spawn(&mut self, f: impl Future<Output = ()> + 'static) {
        self.tasks_queue.push(Task::new(f));
    }

    fn run(&mut self) {
        'update_loop: loop {
            let frame_start = Instant::now();

            'current_frame: loop {
                let task = self.tasks_queue.pop();

                match task {
                    None => break 'current_frame,
                    Some(mut task) => {
                        let task_id = self.task_id_counter;

                        let waker = TaskWaker::waker(task_id, self.sender.clone());

                        match task.poll(Context::from_waker(&waker)) {
                            Poll::Ready(()) => (),
                            Poll::Pending => {
                                // taskをBTreeに入れる
                                self.wait_tasks.insert(task_id, task);
                                self.task_id_counter += 1;
                            }
                        }

                        // 先のpollでwakerが呼ばれていた分のreceiverをチェック
                        while let Ok(Some(task_id)) = self.receiver.try_next() {
                            let t = self.wait_tasks.remove(&task_id).unwrap();
                            self.tasks_queue.push(t);
                        }
                    }
                }
            }

            // 1/60待つ
            let now = Instant::now();
            let duration = now - frame_start;
            if duration < Duration::new(0, 16666666) {
                sleep(Duration::new(0, 16666666) - duration);
            }

            self.frame_counter += 1;

            if self.wait_tasks.is_empty() {
                break 'update_loop;
            }

            // 1フレーム進んだことをすべてのタスクに通知したい。

            //mspcに溜まっているものを全部洗う。
            while let Ok(Some(task_id)) = self.receiver.try_next() {
                let t = self.wait_tasks.remove(&task_id).unwrap();
                self.tasks_queue.push(t);
            }

            // // wait_tasksの中からwakedなタスクだけをtasksに入れる。
            // // let mut i = 0;
            // // while i == self.wait_tasks.len() {
            // //     if self.wait_tasks[&i].waked {
            // //         self.tasks_queue.push(self.wait_tasks.remove(&i).expect(""));
            // //     } else {
            // //         i += 1;
            // //     }
            // // }
            // let mut waked_index = vec![];
            // for (i, t) in self.wait_tasks.iter() {
            //     if t.waked {
            //         waked_index.push(i);
            //     }
            // }
        }
    }
}

#[derive(Clone)]
struct TaskWaker {
    task_id: TaskId,
    sender: Arc<Mutex<Sender<TaskId>>>,
}
impl TaskWaker {
    fn waker(task_id: TaskId, sender: Sender<TaskId>) -> Waker {
        futures::task::waker(Arc::new(Self {
            task_id,
            sender: Arc::new(Mutex::new(sender)),
        }))
    }
}
impl ArcWake for TaskWaker {
    fn wake_by_ref(arc_self: &Arc<Self>) {
        arc_self
            .sender
            .lock()
            .unwrap()
            .start_send(arc_self.task_id)
            .unwrap();
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

    fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
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

pub fn main_v3() {
    let mut rt = Runtime::new();
    rt.spawn(async {
        ten_frame_task(0).await;
        ten_frame_task(1).await;
        let t2 = ten_frame_task(2);
        let t3 = ten_frame_task(3);
        join!(t2, t3);
        ten_frame_task(4).await;
    });
    rt.run();
}
