use std::collections::VecDeque;
use std::io::Write;
use std::ops::{Deref,DerefMut};
use std::sync::atomic::{AtomicBool,Ordering};
use std::cell::UnsafeCell;
use std::sync::Arc;
use std::task::{Context, Waker,Poll};
use async_timer::AsyncTimeout;
use futures::{Future};
use futures::executor::block_on;

mod async_timer;


struct PseudoGuard<'a, T>
{
    lock : &'a PseudoMutex<T>
}

impl<'a, T> PseudoGuard<'a,T>
{   #[allow(dead_code)]
    fn new(pseud : &'a PseudoMutex<T>) -> PseudoGuard<'a,T>
    {
        PseudoGuard { lock: pseud }//, poisoned: PoisonError::<T>::ok() }
    }
}

impl<T> Drop for PseudoGuard<'_,T>
{
    fn drop(&mut self)
    {
        self.lock.release();
    }
}



impl<T> Deref for PseudoGuard<'_,T>
{
    type Target = T;
    fn deref(&self) -> &Self::Target 
    {
        unsafe{&*self.lock.data.get()}
    }
}

impl<T> DerefMut for PseudoGuard<'_,T>{
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.lock.data.get()}
    }
}


struct FutureLock<'a, T>
{
    source : &'a PseudoMutex<T>,
    context_in_mtx : AtomicBool,
}
impl<'a,T> FutureLock<'a,T>
{
    fn new( _source : &'a PseudoMutex<T>) -> FutureLock<'a,T>
    {
        FutureLock { 
             source: _source,
             context_in_mtx: AtomicBool::new(false),
             }
    }
}

impl<'a,T> Future for FutureLock<'a,T>
{
    type Output = PseudoGuard<'a,T>;

    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> 
    { 
        if (self.context_in_mtx.load(Ordering::Relaxed) == false)
        {
            self.source.queue_waker(cx.waker().clone());
            self.context_in_mtx.store(true, Ordering::Relaxed);
            Poll::Pending
        }

        else
        {
            
            match self.source.bool_locked.compare_exchange(false,true, Ordering::Relaxed, Ordering::Relaxed)
                {
                    Ok(_) => Poll::Ready(PseudoGuard::new(self.source)),
                    Err(_) => Poll::Pending
                }
        }
    }
}


struct PseudoMutex<T>
{
    bool_locked : AtomicBool,
    can_queue : AtomicBool,
    task_queue : UnsafeCell::<VecDeque::<Waker>>,
    data : UnsafeCell<T>,
    //poison : bool
}
unsafe impl<T> Send for PseudoMutex<T> {}
unsafe impl<T> Sync for PseudoMutex<T> {}



impl<T> PseudoMutex<T>
{
    
    fn new(resource :T) ->PseudoMutex<T>
    {

        let deq = VecDeque::<Waker>::with_capacity(16);
        PseudoMutex::<T>
        {
            bool_locked : AtomicBool::new(false),
            can_queue : AtomicBool::new(true),
            task_queue : UnsafeCell::new(deq),
            data: UnsafeCell::<T>::new(resource),
            //poison : false
         }
        
        
    }
    
    

    async fn lock(&self) -> PseudoGuard<T>
    {
        let lockwaiter = FutureLock::new(self);
        lockwaiter.await
    }




    fn queue_waker(& self, wk : Waker)
    {
        unsafe
        {
            let queueref = &mut *self.task_queue.get();
            while self.can_queue.compare_exchange(true,false, Ordering::Relaxed, Ordering::Relaxed).is_err()
                {continue;}           

                queueref.push_back(wk.clone());
            if queueref.len()==1 {wk.clone().wake()}
            self.can_queue.store(true,Ordering::Relaxed);
        }
        
    }

    

    
    fn release(&self) -> ()
    {
        if self.bool_locked.load(Ordering::Relaxed)==false {return;} //in case of double call (manual + release from dropping MutexGuard)
        unsafe
        {
            let mut queueref = &mut *self.task_queue.get();
            while self.can_queue.compare_exchange(true,false, Ordering::Relaxed, Ordering::Relaxed).is_err()
                {continue;}
            let _x = queueref.pop_front();

            self.bool_locked.store(false,Ordering::Relaxed);
            if queueref.len()!=0 { queueref[0].clone().wake(); }
            self.can_queue.store(true,Ordering::Relaxed);
        }
    }

}
        

async fn mut_work(m : Arc::<PseudoMutex<u32>>, wait : u64, msg :String)
{
    AsyncTimeout::sleep_ms(wait+ rand::random::<u64>()%60).await;
    let mut lo = m.lock().await;
    *lo +=2;
    AsyncTimeout::sleep_ms(wait).await;
    unsafe{
        let qref = &*m.task_queue.get();
    println!("{} - toral {} - q size {} \r",msg,*lo, qref.len());
    //std::io::stdout().flush();
    }
}

async fn  as_main(metroid : Arc::<PseudoMutex<u32>>, msg : String)
{

  let mut futs  = Vec::<_>::new();
  
  for i in 0..200 
  {
    futs.push(mut_work(metroid.clone(),20,msg.clone()));
  }
  futures::future::join_all(futs).await;

}


fn main()
{
    let mutax = Arc::new(PseudoMutex::<u32>::new(11));
    let mut handvec = Vec::<_>::new();
    for i in 0..64
    {
        let r: Arc<PseudoMutex<u32>> = mutax.clone();
        handvec.push(std::thread::spawn( move ||{block_on(as_main(r,format!("thread {}",i)))}));
    }

    for hands in handvec {hands.join();}
}

