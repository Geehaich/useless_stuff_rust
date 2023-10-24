use futures::executor::block_on;
use std::sync::Arc;
use crate::PseudoMutex;




pub mod async_timer
{
///Trivial and unoptimized implementation of an asynchronous timer, used for benchmarking pseudo mutexes
/// 
use std::time::{Instant,Duration};
use std::future::Future;
use std::pin::Pin;
use std::task::{Poll,Context}; 
pub struct AsyncTimeout
{
    target_time : Instant
}

impl AsyncTimeout
{
    #[allow(dead_code)]
    pub fn sleep_duration(targ_d : Duration) -> AsyncTimeout
    {
        AsyncTimeout{target_time : Instant::now() + targ_d }
    }
    #[allow(dead_code)]
    pub fn sleep_ms(millis : u64) -> AsyncTimeout
    {
        AsyncTimeout{target_time : Instant::now() + Duration::from_millis(millis)}
    }
    #[allow(dead_code)]
    pub fn sleep_us(micros : u64) -> AsyncTimeout
    {
        AsyncTimeout{target_time : Instant::now() + Duration::from_micros(micros)}
    }

}

impl Future for AsyncTimeout
{
    type Output = bool;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output>
    {
        if  Instant::now() < self.target_time
        { 
            cx.waker().clone().wake();
            Poll::Pending
        }
        else
        { 
            
            Poll::Ready(true) }
    }

}
}



#[allow(dead_code)]
/// Function called by every task in the queue
async fn mut_work(m : Arc::<PseudoMutex<u32>>, wait : u64)
{
    async_timer::AsyncTimeout::sleep_ms(wait+ rand::random::<u64>()%60).await;
    let mut  guard = m.lock().await;
    
    *guard +=rand::random::<u32>()%10+1;
    *guard %= 15000;
    m.release();
}

#[allow(dead_code)]
//equivalent to mut_work using OS mutexes
async fn os_mut_work(m : Arc::<std::sync::Mutex<u32>>, wait : u64)
{
    async_timer::AsyncTimeout::sleep_ms(wait+ rand::random::<u64>()%60).await;
    let mut  guard = m.lock().unwrap();
    *guard +=rand::random::<u32>()%10+1;
    *guard %= 15000;
}

#[allow(dead_code)]
/// asynchronous function spawning n_tasks asynchronous tasks
async fn  as_main(metroid : Arc::<PseudoMutex<u32>>, n_tasks : u32)
{

  let mut futs  = Vec::<_>::new();
  
  for _i in 0..n_tasks  
  {
    futs.push(mut_work(metroid.clone(),35));
  }
  futures::future::join_all(futs).await;

}


#[allow(dead_code)]
// equivalent to as_main using OS mutexes instead of PseudoMutex instances
async fn  os_main(metroid : Arc::<std::sync::Mutex<u32>>, n_tasks : u32)
{

  let mut futs  = Vec::<_>::new();
  
  for _i in 0..n_tasks 
  {
    futs.push(os_mut_work(metroid.clone(),35));
  }
  futures::future::join_all(futs).await;

}

#[allow(dead_code)]
// benchmark function. Takes a mutex owning a u32, increments it using asynchronous tasks across
// multiple threads, and returns computation times of this task and an equivalent relying on OS mutexes.
pub fn bench_as_vs_os (threads : u32 , tasks : u32) -> (u128 , u128)
{
    let mutax = Arc::new(PseudoMutex::<u32>::new(0));
    let mut handvec = Vec::<_>::new();

    let a = std::time::Instant::now();
    for _i in 0..threads
    {
        let r: Arc<PseudoMutex<u32>> = mutax.clone();
        handvec.push(std::thread::spawn( move ||{block_on(as_main(r,tasks))}));
    }

    for hand in handvec{let _= hand.join();}

    let as_time = a.elapsed().as_micros();


    let a = std::time::Instant::now();
    let osax = Arc::new(std::sync::Mutex::<u32>::new(0));
    let mut handvec = Vec::<_>::new();

    for _i in 0..threads
    {
        let r  = osax.clone();
        handvec.push(std::thread::spawn( move ||{block_on(os_main(r,tasks))}));
    }

    for hand in handvec{let _= hand.join();}

    let os_time = a.elapsed().as_micros();

    (as_time,os_time)

}




// same as mut_work with printouts
async fn verbose_mut_work(m : Arc::<PseudoMutex<u32>>, wait : u64, thread : u32, task : u32)
{
    async_timer::AsyncTimeout::sleep_ms(wait+ rand::random::<u64>()%60).await;
    let mut  guard = m.lock().await;
    println!("thread {} , task {} acquired mutex, current value {}", thread,task,*guard);
    *guard +=rand::random::<u32>()%10+1;
    *guard %= 15000;

    async_timer::AsyncTimeout::sleep_ms(wait).await;

    println!("tout");


    m.release();


}

async fn  verbose_as_main(metroid : Arc::<PseudoMutex<u32>>,n_thread : u32, n_tasks : u32, wait : u64)
{

  let mut futs  = Vec::<_>::new();
  
  for _i in 0..n_tasks  
  {
    futs.push(verbose_mut_work(metroid.clone(),wait as u64,n_thread,_i));
  }
  futures::future::join_all(futs).await;

}




#[allow(dead_code)]
pub fn bench_tasks_threads(threads : u32, tasks : u32, wait : u64)
{
    let mutax = Arc::new(PseudoMutex::<u32>::new(0));
    let mut handvec = Vec::<_>::new();

    let _ = std::time::Instant::now();
    for _i in 0..threads
    {
        let r: Arc<PseudoMutex<u32>> = mutax.clone();
        handvec.push(std::thread::spawn( move ||{block_on(verbose_as_main(r,_i,tasks,wait))}));
    }

    for hand in handvec{let _= hand.join();}

}
