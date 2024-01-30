use crate::Mutex;
use futures::executor::block_on;
use std::{sync::Arc, time::Duration};
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

#[allow(dead_code)]
/// Function called by every task in the queue
async fn mut_work(m: Arc<Mutex<u32>>, wait: u64) {
    async_timer::AsyncTimeout::sleep_ms(wait + rand::random::<u64>() % 60).await;
    let mut guard = m.lock().await;

    *guard += rand::random::<u32>() % 10 + 1;
    *guard %= 15000;
    m.release();
}

#[allow(dead_code)]
//equivalent to mut_work using OS mutexes
async fn os_mut_work(m: Arc<std::sync::Mutex<u32>>, wait: u64) {
    async_timer::AsyncTimeout::sleep_ms(wait + rand::random::<u64>() % 60).await;
    let mut guard = m.lock().unwrap();
    *guard += rand::random::<u32>() % 10 + 1;
    *guard %= 15000;
}

#[allow(dead_code)]
/// asynchronous function spawning n_tasks asynchronous tasks
async fn as_main(metroid: Arc<Mutex<u32>>, n_tasks: u32) {
    let mut futs = Vec::<_>::new();

    for _i in 0..n_tasks {
        futs.push(mut_work(metroid.clone(), 35));
    }
    futures::future::join_all(futs).await;
}

#[allow(dead_code)]
// equivalent to as_main using OS mutexes instead of PseudoMutex instances
async fn os_main(metroid: Arc<std::sync::Mutex<u32>>, n_tasks: u32) {
    let mut futs = Vec::<_>::new();

    for _i in 0..n_tasks {
        futs.push(os_mut_work(metroid.clone(), 35));
    }
    futures::future::join_all(futs).await;
}

#[allow(dead_code)]
// benchmark function. Takes a mutex owning a u32, increments it using asynchronous tasks across
// multiple threads, and returns computation times of this task and an equivalent relying on OS mutexes.
pub fn bench_as_vs_os(threads: u32, tasks: u32) -> (u128, u128) {
    let mutax = Arc::new(Mutex::<u32>::new(0));
    let mut handvec = Vec::<_>::new();

    //Custom mutex
    let a = std::time::Instant::now();
    for _i in 0..threads {
        let r: Arc<Mutex<u32>> = mutax.clone();
        handvec.push(std::thread::spawn(move || block_on(as_main(r, tasks))));
    }

    for hand in handvec {
        let _ = hand.join();
    }

    let as_time = a.elapsed().as_micros();

    //OS mutex
    let a = std::time::Instant::now();
    let osax = Arc::new(std::sync::Mutex::<u32>::new(0));
    let mut handvec = Vec::<_>::new();

    for _i in 0..threads {
        let r = osax.clone();
        handvec.push(std::thread::spawn(move || block_on(os_main(r, tasks))));
    }

    for hand in handvec {
        let _ = hand.join();
    }

    let os_time = a.elapsed().as_micros();

    (as_time, os_time)
}

// same as mut_work with printouts
async fn verbose_mut_work(m: Arc<Mutex<u32>>, wait: u64, thread: u32, task: u32) {
    async_timer::AsyncTimeout::sleep_ms(wait + rand::random::<u64>() % 60).await;
    let mut guard = m.lock().await;
    println!(
        "thread {} , task {} acquired mutex, current value {}",
        thread, task, *guard
    );
    *guard += rand::random::<u32>() % 10 + 1;
    *guard %= 15000;

    async_timer::AsyncTimeout::sleep_ms(wait).await;

    println!("tout");

    m.release();
}

async fn verbose_as_main(metroid: Arc<Mutex<u32>>, n_thread: u32, n_tasks: u32, wait: u64) {
    let mut futs = Vec::<_>::new();

    for _i in 0..n_tasks {
        futs.push(verbose_mut_work(metroid.clone(), wait as u64, n_thread, _i));
    }
    futures::future::join_all(futs).await;
}

#[allow(dead_code)]
pub fn bench_tasks_threads(threads: u32, tasks: u32, wait: u64) {
    let mutax = Arc::new(Mutex::<u32>::new(0));
    let mut handvec = Vec::<_>::new();

    let _ = std::time::Instant::now();
    for _i in 0..threads {
        let r: Arc<Mutex<u32>> = mutax.clone();
        handvec.push(std::thread::spawn(move || {
            block_on(verbose_as_main(r, _i, tasks, wait))
        }));
    }

    for hand in handvec {
        let _ = hand.join();
    }
}

//benchmark things
#[test]
fn single() {
    let lim : u128 = 10;
    let mut r : (u128,u128) = (0,0);
    for i in 0..lim {
        let t : (u128, u128) = bench_as_vs_os(8, 200);
        r.0 += t.0;
        r.1 += t.1;
        println!("Loop {} Crate: {}, OS {}; ", i, t.0, t.1);
    }

    println!("Total - Crate: {} OS: {}", r.0 / lim, r.1 / lim)
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


#[test]
fn from_test() //test conversion from std mutex
{
    let mutex_os: std::sync::Mutex<u64>  = crate::Sync_Mutex::new(0);
    let mutex_as : Mutex<u64> = Mutex::<u64>::from(mutex_os);
    let marc = std::sync::Arc::new(mutex_as);

    let mut handles
     = vec![];

    let n_threads = 80;
    for i in 0..n_threads {
        let data = std::sync::Arc::clone(&marc);
        handles.push(std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(i*3^113)); //scrambles thread access order
            let mut guard = data.sync_lock();
            *guard += i;
            println!("thread {} has mtx",i);
            // the lock is unlocked here when `data` goes out of scope.
        }));
    }
    for hand in handles {
        _ = hand.join();
    }
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

    let lim: usize = 100_000;
    (
        async {
            for _i in 0..lim {
                mtx.lock().await.field1 += 1;
                std::thread::sleep(Duration::from_micros(5));
            }
        },
        async {
            for j in 0..lim {
                let c = char::from_u32(j as u32 % 26 + 65).unwrap();
                mtx.lock().await.field2.push(c);
                std::thread::sleep(Duration::from_micros(5));
            }
        },
        async {
            for k in 0..lim {
                mtx.lock().await.field4.push(k);
                std::thread::sleep(Duration::from_micros(5));
            }
        }
    ).join().await;
    dbg!(&*mtx.lock().await);
}

#[allow(dead_code)]
#[tokio::test]
async fn concurrency_test_mixed() {
    let mtx = Arc::new(Mutex::new(Schmilblick::new()));

    let lim: usize = 100_000;
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