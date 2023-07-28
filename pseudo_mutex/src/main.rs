use std::collections::VecDeque;
use std::ops::{Deref,DerefMut};
use std::sync::atomic::{AtomicBool,Ordering};
use std::cell::UnsafeCell;
use std::task::{Context, Waker,Poll};
use futures::Future;


struct WakeQueue
{
    waker_queue : VecDeque<(Waker,u32)>
}

impl WakeQueue
{
    fn new()-> WakeQueue {WakeQueue{waker_queue : VecDeque::<(Waker,u32)>::with_capacity(8)}}

    fn len(&self) -> usize {self.waker_queue.len()}

    fn add(&mut self,waks : Waker)
    {
        for wake_tuple in self.waker_queue
        {
            if waks. == wake_tuple.0
            {
                wake_tuple.1 +=1 ;
            }
        }
    }
}


/// Equivalent to MutexGuard, contains a reference to underlying mutex.
/// Releases the mutex when dropped but doesn't support poisoning (similar to tokio implementation)
struct PseudoGuard<'a, T>
{
    lock : &'a PseudoMutex<T>
}

impl<'a, T> PseudoGuard<'a,T>
{   
    ///basic ctor, will be called when FutureLocks are polled on the released mutex
    fn new(pseud : &'a PseudoMutex<T>) -> PseudoGuard<'a,T>
    {
        PseudoGuard { lock: pseud } 
    }
    
}

impl<T> Drop for PseudoGuard<'_,T>
{
    /// the drop implementation frees the mutex and checks for panics but doesn't poison the mutex if one happened while holding the struct
    fn drop(&mut self)
    {
        if std::thread::panicking()
        {
            self.lock.has_panicked.store(true,Ordering::Relaxed);
        }
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


/// FutureLocks are transitory pollable objects who append their wakers to the Mutex's Waker queue on the
/// first poll, and attempt to lock the Mutex and create a PseudoGuard on subsequent polls.
/// This implementation is poorly optimised, several tasks in the same context can have the same waker so the total number of polls is o(N!)
/// where N is the number of concurrent tasks. Severe performance drops are expected for N>6 and RAM required exceeds 12Gb if N>8.
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
        if self.context_in_mtx.load(Ordering::Relaxed) == false //store waker reference in the mutex on first call
        {
            self.source.queue_waker(cx.waker().clone());
            self.context_in_mtx.store(true, Ordering::Relaxed);
            Poll::Pending
        }

        else
        {
            //On subsequent calls, try to lock and obtain a PseudoGuard.
            match self.source.bool_locked.compare_exchange(false,true, Ordering::Relaxed, Ordering::Relaxed)
                {
                    Ok(_) => 
                    {

                            Poll::Ready(PseudoGuard::new(self.source))
                    },
                    Err(_) => 
                    {
                        Poll::Pending
                    }
                }

        }

        //Note that the Context's Waker is only stored, calls to wake() will be done on the Mutex's end on lock release.
    }
}



///Asynchronous Mutex-like object.
/// Unlike a std mutex and like a tokio mutex, it doesn't support data poisoning but will raise a flag if a thread
/// dropped a mutex guard attached to it while panicking.
struct PseudoMutex<T>
{
    bool_locked : AtomicBool,
    can_queue : AtomicBool,
    task_queue : UnsafeCell::<VecDeque::<Waker>>,
    data : UnsafeCell<T>,
    has_panicked : AtomicBool
}
unsafe impl<T> Send for PseudoMutex<T> {}
unsafe impl<T> Sync for PseudoMutex<T> {}



impl<T> PseudoMutex<T>
{
    #[allow(dead_code)]
    fn new(resource :T) ->PseudoMutex<T>
    {

        let deq = VecDeque::<Waker>::with_capacity(16);
        PseudoMutex::<T>
        {
            bool_locked : AtomicBool::new(false),
            can_queue : AtomicBool::new(true),
            task_queue : UnsafeCell::new(deq),
            data: UnsafeCell::<T>::new(resource),
            has_panicked : AtomicBool::new(false)
         }
        
        
    }
    
    #[allow(dead_code)]
    ///asynchronous locking operation. The futureLock object will create a PseudoGuard when waited on.
    async fn lock(&self) -> PseudoGuard<T>
    {

        let lockwaiter = FutureLock::new(self);
        lockwaiter.await
    }



    /// waits until the deque is free and adds a Waker to the back
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


    //remove the first waker in the deque, and call wake() on the next one if applicable.
    fn wake_next(&self)
    {
        unsafe
        {
            let queueref = &mut *self.task_queue.get();
            while self.can_queue.compare_exchange(true,false, Ordering::Relaxed, Ordering::Relaxed).is_err()
                {continue;}
            let _x = queueref.pop_front();

            self.can_queue.store(true,Ordering::Relaxed);

            if queueref.len()!=0 { queueref[0].clone().wake(); }
        }
    }
    

    fn release(&self) -> ()
    {
        match self.bool_locked.compare_exchange(true,false, Ordering::Relaxed, Ordering::Relaxed)
        {
            Ok(_x) => self.wake_next(),
            Err(_e) => return
        }
    }

}
        






fn main()
{
    

}


#[cfg(test)]
mod benchs;
// #[test]
// fn as_os(){benchs::bench_as_vs_os();}

#[test]
fn as_test()
{
    const N_T : usize =  5;
    const N_TH : usize = 4;
    let tasks : [u32;N_T] = [64,128,256,512,1024];
    let thread_count : [u32;N_TH] = [1,2,4,8];
    let mut bench_µs_as : [[u128;N_T];N_TH] = [[0;N_T];N_TH] ; 
    let mut bench_µs_os : [[u128;N_T];N_TH] = [[0;N_T];N_TH] ; 

    for i_th in 0..N_TH
    {
        for j_task in 0..N_T
        {
            for _x in 0..10
            { 
                let (tim_as, tim_os ) = benchs::bench_as_vs_os(thread_count[i_th],tasks[j_task]);
                bench_µs_as [i_th][j_task] += tim_as;
                bench_µs_os [i_th][j_task] += tim_os;
                println!("{}",_x);
            }
            println!("{}",j_task);
            
        }

        for j in 0..N_T
        {
            bench_µs_as [i_th][j] /=10;
            bench_µs_os [i_th][j] /=10;
        }
    }
    println!("{:?}",bench_µs_as);
    println!("{:?}",bench_µs_os);

}

