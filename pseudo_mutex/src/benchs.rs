use crate::Mutex;
use futures::executor::block_on;
use rand::RngCore;
use std::{collections::VecDeque, sync::Arc, time::Duration};
use futures_concurrency::prelude::*;

pub mod async_timer {
    use std::future::Future;
    use std::pin::Pin;
    use std::task::{Context, Poll};

    ///Trivial and unoptimized implementation of an asynchronous timer, used for benchmarking pseudo mutexes
    use std::time::{Duration, Instant};
    pub struct AsyncTimeout {
        target_time: Instant,
    }

    #[allow(dead_code)]
    impl AsyncTimeout {
        pub fn sleep_duration(targ_d: Duration) -> AsyncTimeout {
            AsyncTimeout {
                target_time: Instant::now() + targ_d,
            }
        }

        pub fn sleep_ms(millis: u64) -> AsyncTimeout {
            AsyncTimeout {
                target_time: Instant::now() + Duration::from_millis(millis),
            }
        }

        pub fn sleep_us(micros: u64) -> AsyncTimeout {
            AsyncTimeout {
                target_time: Instant::now() + Duration::from_micros(micros),
            }
        }
    }

    impl Future for AsyncTimeout {
        type Output = bool;
        fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            if Instant::now() < self.target_time {
                cx.waker().clone().wake();
                Poll::Pending
            } else {
                Poll::Ready(true)
            }
        }
    }
}

async fn mut_work(m : Arc<Mutex<(VecDeque<f32>,f32)>>,thread_index:u32)
{
    let mut guard = m.lock().await;


    let mut mean_init =0.0;
    for i in 0..guard.0.len() {mean_init += guard.0[i].log10();}
    mean_init = mean_init / guard.0.len() as f32;

    assert_eq!(mean_init, guard.1);

    guard.0.push_back ( (rand::thread_rng().next_u32()%30 +1) as f32);
    if guard.0.len()> 32 {guard.0.pop_front();}

    let mut mean_end =0.0;
    for i in 0..guard.0.len() {mean_end += guard.0[i].log10();}
    mean_end = mean_end / guard.0.len() as f32;

    guard.1 = mean_end;

}

async fn as_main(metroid: Arc<Mutex<(VecDeque<f32>,f32)>>, n_tasks: u32, i:u32) {

    let mut futs = Vec::<_>::new();
    for _i in 0..n_tasks
    {
        futs.push(mut_work(metroid.clone(),i));
    }
    futures::future::join_all(futs).await;
}


fn basic_test()
{
    let mut container_test = VecDeque::<f32>::with_capacity(16);
    container_test.push_front(1.0);
    let mut avg: f32 = 0.0;
    let mut mut_deq = Arc::new(Mutex::new((container_test , avg)));

    let mut handvec = Vec::<_>::new();

    let N_THREADS = 16;
    let N_TASKS = 400;
    for i in 0..N_THREADS
    {
        let thread_ref = mut_deq.clone();
        handvec.push(std::thread::spawn(move || block_on(as_main(thread_ref, N_TASKS,i))));

    }
    for hand in handvec 
    {
        let _ = hand.join();
    }


}

#[tokio::test]
async fn loop_basic()
{
    for i in 0..2000
    {
        basic_test();
        println!("loop {}",i);
    };
}



#[test]
fn block_lock()  //test blocking access
{
    let mutax  = std::sync::Arc::new(Mutex::new(0));
    let mut handles = vec![];

    let n_threads = 25;
    for i in 0..n_threads {
        let data: Arc<Mutex<u64>> = std::sync::Arc::clone(&mutax);
        handles.push(std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis((10*i)^113)); //scrambles thread access order
            let mut guard = data.sync_lock();
            *guard += i;
            println!("thread {} has mtx",i);
            println!("{}",*guard);
            // the lock is unlocked here when `data` goes out of scope.
        }));
       }

    for hand in handles { _ = hand.join();}
}



#[derive(Debug)]
struct Schmilblick {
    field1 : i64,
    field2 : String,
    field3 : (u128,bool),
    field4 : Vec<usize>
}

impl Schmilblick {
    pub fn new() -> Self {
        Self {
            field1 : 1,
            field2 : String::from("foo"),
            field3 : (2,true),
            field4 : vec!(4,5,6)
        }
    }
}

#[allow(dead_code)]
#[tokio::test]
async fn concurrency_test_single() {
    let mtx = Arc::new(Mutex::new(Schmilblick::new()));

    let t = 5u64;
    let lim: usize = 100;
    (
        async {
            for _i in 0..lim {
                mtx.lock().await.field1 += 1;
                std::thread::sleep(Duration::from_micros(t));
            }
        },
        async {
            for j in 0..lim {
                let c = char::from_u32(j as u32 % 26 + 65).unwrap();
                mtx.lock().await.field2.push(c);
                std::thread::sleep(Duration::from_micros(t));
            }
        },
        async {
            for k in 0..lim {
                mtx.lock().await.field4.push(k);
                std::thread::sleep(Duration::from_micros(t));
            }
        }
    ).join().await;
    dbg!(&*mtx.lock().await);
}

#[allow(dead_code)]
#[tokio::test]
async fn concurrency_test_mixed() {
    let mtx = Arc::new(Mutex::new(Schmilblick::new()));

    let lim: usize = 50;
    (
        async {
            for _i in 0..lim {
                dbg!("Task A");
                {
                    let mut guard = mtx.lock().await;
                    guard.field1 += 1;
                    let sz = guard.field4.len() - 1;
                    guard.field4[sz] = usize::MAX;
                }
            }
        },
        async {
            for j in 0..lim {
                dbg!("Task B");
                let c = char::from_u32(j as u32 % 26 + 65).unwrap();
                mtx.lock().await.field2.push(c);
                mtx.lock().await.field4.pop();
            }
        },
        async {
            for k in 0..lim {
                dbg!("Task C");
                {
                    let mut guard = mtx.lock().await;
                    guard.field4.push(k);
                    guard.field3.1 = !guard.field3.1;
                }
            }
        }
    ).join().await;
    dbg!(&*mtx.lock().await);
}

async fn async_test_part(mtx : Arc<Mutex<Schmilblick>>) {
    let lim: usize = 1000;
    (
        async {
            for _i in 0..lim {
                {
                    let mut guard = mtx.lock().await;
                    guard.field1 += 1;
                    let sz = guard.field4.len() - 1;
                    guard.field4[sz] = usize::MAX;
                }
            }
        },
        async {
            for j in 0..lim {
                let c = char::from_u32(j as u32 % 26 + 65).unwrap();
                mtx.lock().await.field2.push(c);
                mtx.lock().await.field4.pop();
            }
        },
        async {
            for k in 0..lim {
                {
                    let mut guard = mtx.lock().await;
                    guard.field4.push(k);
                    guard.field3.1 = !guard.field3.1;
                }
            }
        }
    ).join().await;
    println!("End task");
}

fn sync_async(mtx : Arc<Mutex<Schmilblick>>) -> Box<dyn futures::Future<Output = ()>>{
    Box::new(async_test_part(mtx))
}

// Test not complete for the momemt... DO NOT USE
#[allow(dead_code)]
#[tokio::test]
async fn concurency_test_thread() {


    let threads : Vec<std::thread::JoinHandle<Box<dyn futures::Future<Output = ()>>>> = Vec::with_capacity(32);
    let mtx = Arc::new(Mutex::new(Schmilblick::new()));

    //Spawn thread with task
    for _i in 0..20 {
        //let task = Box::<dyn futures::Future<Output = ()>>::new(async_test_part(mtx.clone()));
        let clonx = mtx.clone();
        let _task = sync_async(clonx);


        //threads.push(std::thread::spawn(|| task)); // TODO debug
    }

    //Join thread
    for thd in threads {
        let _ = thd.join();
    }
}