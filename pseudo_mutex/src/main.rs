use std::collections::VecDeque;
use std::ops::{Deref,DerefMut};
use std::sync::atomic::{AtomicBool,Ordering};
use std::cell::UnsafeCell;
use std::task::{Context, Waker,Poll};
use futures::Future;




//PseudoMutex is an asynchronous equivalent to a Mutex using a Deque to keep track of tasks waiting on the resource.
struct PseudoMutex<T>
{
    bool_locked : AtomicBool,
    can_queue : AtomicBool,
    task_queue : UnsafeCell::<VecDeque::<Waker>>,
    data : UnsafeCell<T>
}
unsafe impl<T> Send for PseudoMutex<T> {}
unsafe impl<T> Sync for PseudoMutex<T> {}



impl<T> PseudoMutex<T>
{
    #[allow(dead_code)]
    //ctor setting deque initial capacity
    fn new_with_capacity(resource :T, cap : usize) ->PseudoMutex<T>
    {

        let deq = VecDeque::<Waker>::with_capacity(cap);
        PseudoMutex::<T>
        {
            bool_locked : AtomicBool::new(false),
            can_queue : AtomicBool::new(true),
            task_queue : UnsafeCell::new(deq),
            data: UnsafeCell::<T>::new(resource)
         }
        
        
    }
    #[allow(dead_code)]
    //ctor with default capacity of 8, should cover most cases
    fn new(resource :T) ->PseudoMutex<T>
    {
        return PseudoMutex::<T>::new_with_capacity(resource,8);
    }
    
    #[allow(dead_code)]
    /// Lock the mutex by creating a FutureLock structs and awaiting it. A successful FutureLock poll locks the atomic lock and returns a PseudoGuard used to 
    /// access the resource
    async fn lock(&self) -> PseudoGuard<T>
    {
        let lockwaiter = FutureLock::new(self);
        lockwaiter.await
    }



    /// pushes a Waker linked to a FutureLock to the front of the queue. Wakes it if it's the only one there, giving the resource to the task.
    fn queue_waker(& self, wk : Waker)
    {
        unsafe
        {

            let queueref = &mut *self.task_queue.get();
            // Wait on insertion lock to access the deque
            while self.can_queue.compare_exchange(true,false, Ordering::Relaxed, Ordering::Relaxed).is_err()
                {continue;}           

                queueref.push_back(wk.clone());
            if queueref.len()==1 {wk.clone().wake()}
            self.can_queue.store(true,Ordering::Relaxed); //  unlock the deque again
        }
        
    }

    

    // unlock the mutex, start next Waker to attribute the resource if applicable
    fn release(&self) -> ()
    {
        if self.bool_locked.load(Ordering::Relaxed)==false {return;} //in case of double call (manual + release from dropping MutexGuard)
        unsafe
        {
            let queueref = &mut *self.task_queue.get();
            while self.can_queue.compare_exchange(true,false, Ordering::Relaxed, Ordering::Relaxed).is_err()
                {continue;}
            let _x = queueref.pop_front();

            self.bool_locked.store(false,Ordering::Relaxed);
            if queueref.len()!=0 { queueref[0].clone().wake(); }
            self.can_queue.store(true,Ordering::Relaxed);
        }
    }

}



///Futurelocks are an intermediary struct necessary to make the PseudoMutex locking asynchronous,
/// we need an element implementing future to allow waiting for a PseudoGuard and collect references to the Waker used.
/// They work by adding a reference to the current Waker to the PseudoMutex queue on the first poll
/// and checking for the PseudoMutex lock on subsequent calls.
struct FutureLock<'a, T>
{
    source : &'a PseudoMutex<T>,
    context_in_mtx : AtomicBool
}
impl<'a,T> FutureLock<'a,T>
{
    // trivial ctor
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
        // on first poll, add the waker to PseudoMutex queue
        if self.context_in_mtx.load(Ordering::Relaxed) == false
        {
            self.source.queue_waker(cx.waker().clone());
            self.context_in_mtx.store(true, Ordering::Relaxed);
            Poll::Pending
        }

        else
        {
            //check if locked by attempting to lock. if success return Pseudoguard
            match self.source.bool_locked.compare_exchange(false,true, Ordering::Relaxed, Ordering::Relaxed)
                {
                    Ok(_) => Poll::Ready(PseudoGuard::new(self.source)),
                    Err(_) => Poll::Pending
                }
        }
    }
}




///PseudoGuard is the equivalent of a Mutexguard, a struct used to access the data in a PseudoMutex which unlocks it on Drop.
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


//Deref and DerefMut return references to the actual resource. they're fundamentally unsafe as the resource
// is behing an UnsafeCell<T> but are implemented safe to comply with trait specs
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





fn main()
{
    

}


#[cfg(test)]
mod benchs;



//benchmark things
#[test]
fn single()
{
    benchs::bench_tasks_threads(2,50,15);
}
