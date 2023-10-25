use core::future::Future;
use core::cell::UnsafeCell;
use std::collections::VecDeque;
use std::error::Error;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicBool, Ordering};
use core::task::{Context, Poll, Waker};

use std::sync::Mutex as Sync_Mutex;

const DEFAULT_SIZE: usize = 16;

//PseudoMutex is an asynchronous equivalent to a Mutex using a Deque to keep track of tasks waiting on the resource.
pub struct Mutex<T> {
    bool_locked: AtomicBool,
    can_queue: AtomicBool,
    task_queue: UnsafeCell<VecDeque<Waker>>,
    data: UnsafeCell<T>,
}

/// Because UnsafeCell requires unsafe implementations of Send and Sync
unsafe impl<T> Send for Mutex<T> {}
unsafe impl<T> Sync for Mutex<T> {}




/// Mutex: Utilization simillar to the std mutex. Can register multiple waker.
#[allow(dead_code)]
impl<T> Mutex<T> {
    // ctor setting deque initial capacity
    /// Usefull in the case you want more waker than **default size (16)** count at start.
    /// If len = 0, initialise with default size
    pub fn new_with_capacity(resource: T, len: usize) -> Mutex<T> {
        let deque: VecDeque<Waker> = match len > 0 {
            true => VecDeque::<Waker>::with_capacity(len),
            false => VecDeque::<Waker>::with_capacity(DEFAULT_SIZE),
        };

        Mutex::<T> {
            bool_locked: AtomicBool::new(false),    //resou
            task_queue: UnsafeCell::new(deque),     //waker deque. Unsaferce lock
            can_queue: AtomicBool::new(true),       //deque access lockCell justified by need to mutate self across tasks through immutable references.
            data: UnsafeCell::<T>::new(resource),   // Actual resource. UnsafeCell justified because Mozilla does it
        }
    }

    /// Ctor with default capacity of 8, should cover most cases
    pub fn new(resource: T) -> Mutex<T> {
        return Mutex::<T>::new_with_capacity(resource, 0);
    }

    /// Lock the mutex by creating a FutureLock structs (private structure) and awaiting it.
    /// A successful FutureLock poll locks the atomic lock and returns a PseudoGuard used to access the resource
    pub fn lock(&self) -> FutureLock<T> {
        FutureLock::new(self)
    }

    pub fn try_lock(&self) -> Result<LockGuard<T>, LockError>
    {
        match self.bool_locked.compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed) 
        {
            Ok(_) => Ok(LockGuard::<'_,T>::new(self)),
            Err(_) => 
            unsafe{
                Err(LockError::new((*(self.task_queue.get())).len()))
            }
        }
    }

    pub fn sync_lock(&self, wait_granularity : u64 ) -> LockGuard<T>
    {
        while self.bool_locked.compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed).is_err() {
            std::thread::sleep( std::time::Duration::from_micros(wait_granularity)); }
        LockGuard::<'_,T>::new(&self)
    }


    /// Pushes a Waker linked to a FutureLock to the front of the queue. Wakes it if it's the only one there, giving the resource to the task.
    fn queue_waker(&self, wk: Waker) {
        //justification : multiple futurelocks share refs to self (have to be immutable) but each needs to mutate self to update queue
        unsafe {
            let queueref: &mut VecDeque<Waker> = &mut *self.task_queue.get();

            // Wait on insertion lock to access the deque
            while self.can_queue.compare_exchange(true,false, Ordering::Relaxed, Ordering::Relaxed).is_err() {
                continue; }
            queueref.push_back(wk.clone());

            if queueref.len() == 1 {
                wk.clone().wake() }

            self.can_queue.store(true,Ordering::Relaxed); //  unlock the deque again
        }
    }

    // unlock the mutex, start next Waker to attribute the resource if applicable
    pub fn release(&self) -> () {
        if !self.bool_locked.load(Ordering::Relaxed) {
            return; //in case of double call (manual + release from dropping MutexGuard)
        }

        // Justification : same as queue_waker
        unsafe {
            let queueref: &mut VecDeque<Waker> = &mut *self.task_queue.get();
            while self.can_queue.compare_exchange(true,false, Ordering::Relaxed, Ordering::Relaxed).is_err() {
                continue; }

            _ = queueref.pop_front();
            self.bool_locked.store(false,Ordering::Relaxed);

            if queueref.len()!=0 {
                queueref[0].clone().wake(); }

            self.can_queue.store(true,Ordering::Relaxed);
        }
    }
}


impl<T> From<Sync_Mutex<T>> for Mutex<T>
{
    fn from(sync_mtx: Sync_Mutex<T>) -> Self {
        let data = sync_mtx.into_inner().unwrap();
        Mutex::new(data)
    }
}


#[derive(Debug)]
pub struct LockError //Simple error type for the try_error function
{
    n_tasks : usize
}

impl LockError
{
    pub fn new(tasks_ : usize) -> LockError {LockError { n_tasks: tasks_ }}
}

impl core::fmt::Display for LockError
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Error Locking Mutex, {} other tasks in queue", self.n_tasks)
    }
}

impl Error for LockError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {None}
}


/// Futurelocks are an intermediary struct necessary to make the PseudoMutex locking asynchronous,
/// we need an element implementing future to allow waiting for a PseudoGuard and collect references to the Waker used.
/// They work by adding a reference to the current Waker to the PseudoMutex queue on the first poll
/// and checking for the PseudoMutex lock on subsequent calls.
pub struct FutureLock<'a, T> {
    source: &'a Mutex<T>,
    context_in_mtx: AtomicBool,
}

impl<'a, T> FutureLock<'a, T> {
    /// trivial ctor
    fn new(_source: &'a Mutex<T>) -> FutureLock<'a, T> {
        FutureLock {
            source: _source,
            context_in_mtx: AtomicBool::new(false),
        }
    }
}

impl<'a, T> Future for FutureLock<'a, T> {
    type Output = LockGuard<'a, T>;

    /// Try to lock mutex
    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // on first poll, add the waker to PseudoMutex queue
        if !self.context_in_mtx.load(Ordering::Relaxed) {
            self.source.queue_waker(cx.waker().clone());
            self.context_in_mtx.store(true, Ordering::Relaxed);
            Poll::Pending
        } else {
            //check if locked by attempting to lock. if success return Pseudoguard
            match self.source.bool_locked.compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => Poll::Ready(LockGuard::new(self.source)),
                Err(_) => Poll::Pending,
            }
        }
    }
}

///LockGuard is the equivalent of a Mutexguard, a struct used to access the data in a PseudoMutex which unlocks it on Drop.
pub struct LockGuard<'a, T> {
    lock: &'a Mutex<T>,
}

impl<'a, T> LockGuard<'a, T> {
    #[allow(dead_code)]
    /// Ctor, lock pseudo mutex given n argument
    pub fn new(mtx: &'a Mutex<T>) -> LockGuard<'a, T> {
        LockGuard { lock: mtx }
    }
}



impl<T> Drop for LockGuard<'_, T> {
    /// Drop resource (release pseudo mutex)
    fn drop(&mut self) {
        self.lock.release();
    }
}

impl<T> Deref for LockGuard<'_, T> {
    type Target = T;
    /// Deref return reference to the actual resource.
    /// It's fundamentally unsafe as the resource is behing an UnsafeCell<T> but are implemented safe to comply with trait specs
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.data.get() }
    }
}

impl<T> DerefMut for LockGuard<'_, T> {
    /// DerefMut return a mutable reference to the actual ressource.
    /// It's fundamentally unsafe as the resource is behing an UnsafeCell<T> but are implemented safe to comply with trait specs
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.lock.data.get() }
    }
}


#[cfg(test)]
mod benchs;

//benchmark things
#[test]
fn single() {
    let lim : u128 = 10;
    let mut r : (u128,u128) = (0,0);
    for _i in 0..lim {
        let t : (u128, u128) = benchs::bench_as_vs_os(8, 200);
        r.0 += t.0;
        r.1 += t.1;
    }

    println!("Crate: {} OS: {}", r.0 / lim, r.1 / lim)
}

#[test]
fn block_lock()  //test blocking access
{
    let mutax  = std::sync::Arc::new(Mutex::new(0));

    let mut handles = vec![];

    let N_THREADS = 25;
    for i in 0..N_THREADS {
        let data = std::sync::Arc::clone(&mutax);
        handles.push(std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis((10*i)^113)); //scrambles thread access order
            let mut guard = data.sync_lock(10);
            *guard += i;
            println!("thread {} has mtx",i);
            println!("{}",*guard);
            // the lock is unlocked here when `data` goes out of scope.
        }));
       }

    for hand in handles { hand.join();}
    


}


#[test]
fn from_test() //test conversion from std mutex
{
    let mutex_os: Sync_Mutex<u64>  = Sync_Mutex::new(0);
    let mutex_as : Mutex<u64> = Mutex::<u64>::from(mutex_os);
    let marc = std::sync::Arc::new(mutex_as);

    let mut handles = vec![];

    let N_THREADS = 80;
    for i in 0..N_THREADS {
        let data = std::sync::Arc::clone(&marc);
        handles.push(std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(i*3^113)); //scrambles thread access order
            let mut guard = data.sync_lock(10);
            *guard += i;
            println!("thread {} has mtx",i);
            // the lock is unlocked here when `data` goes out of scope.
        }));
       }

    for hand in handles { hand.join();}
    


}

