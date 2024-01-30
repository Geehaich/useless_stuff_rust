use core::future::Future;
use core::cell::UnsafeCell;
use std::collections::HashMap;
use std::error::Error;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use core::task::{Context, Poll, Waker};
use std::sync::Mutex as Sync_Mutex;

#[cfg(test)]
mod benchs;

const DEFAULT_SIZE: usize = 16;
const DEFAULT_YIELD: usize = 32;
const DEFAULT_DURATION : u64 = 10;

//PseudoMutex is an asynchronous equivalent to a Mutex using a Deque to keep track of tasks waiting on the resource.
pub struct Mutex<T> {
    bool_locked: AtomicBool,
    can_queue: AtomicBool,
    is_async_locked : AtomicBool,
    task_queue: UnsafeCell<HashMap<usize,Waker>>,
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
        let map : HashMap<usize,Waker> = if len > 0 { HashMap::with_capacity(len) } else { HashMap::with_capacity(DEFAULT_SIZE)};


        Mutex::<T> {
            bool_locked: AtomicBool::new(false),    //resou
            can_queue: AtomicBool::new(true),       //deque access lockCell justified by need to mutate self across tasks through immutable references.
            is_async_locked : AtomicBool::new(false),
            task_queue : UnsafeCell::new(map),
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
        match self.bool_locked.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        {
            Ok(_) => Ok(LockGuard::new(self)),
            Err(_) => unsafe{ Err(LockError::new((*(self.task_queue.get())).len())) }
        }
    }

    /// Attempt to lock a mutex by yielding thread. If it's take to much time, this function use default granularity time
    pub fn sync_lock(&self) -> LockGuard<T>
    {
        return self.sync_lock_with_time(DEFAULT_DURATION);
    }

    /// Attempt to lock a mutex by yielding thread. If it's take to much time, this function use time granularity in microseconds given in argument
    pub fn sync_lock_with_time(&self, wait_granularity : u64 ) -> LockGuard<T>
    {
        let mut i = 0;
        while self.bool_locked.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            if i < DEFAULT_YIELD {
                std::thread::yield_now();
                i += 1;
            }
            else {
                std::thread::sleep( std::time::Duration::from_micros(wait_granularity));
            }
        }
        self.is_async_locked.store(false, Ordering::Relaxed);
        LockGuard::new(&self)
    }


    /// Pushes a Waker linked to a FutureLock to the front of the queue. Wakes it if it's the only one there, giving the resource to the task.
    fn queue_waker(&self, wk: Waker) -> Option<usize> {

        // Wait on insertion lock to access the deque
        while self.can_queue.compare_exchange(true,false, Ordering::Release, Ordering::Relaxed).is_err() {
            continue; }

        //justification : multiple futurelocks share refs to self (have to be immutable) but each needs to mutate self to update queue
        unsafe {
            let queueref: &mut HashMap<usize, Waker> = &mut *self.task_queue.get();

            //Generate a key associate to the waker
            let mut idx : usize = 0;
            while idx < usize::MAX {
                if !queueref.contains_key(&idx) {
                    if queueref.insert(idx, wk.clone()).is_some() {
                        // panic!("Key already exist, concurency race detected")
                        return Option::None;
                    }
                    break;
                }
                idx += 1;
                if idx == usize::MAX {
                    return Option::None; }
            }
            let r_idx: Option<usize> = Some(idx);
            if queueref.len() == 1 {
                wk.clone().wake() }

            self.can_queue.store(true,Ordering::Relaxed);

            //Return key of inserted waker
            r_idx
        }
    }

    // unlock the mutex, start next Waker to attribute the resource if applicable
    pub fn release(&self) -> () {
        if !self.bool_locked.load(Ordering::Relaxed) {
            return; //in case of double call (manual + release from dropping MutexGuard)
        }

        // Justification : same as queue_waker
        while self.can_queue.compare_exchange( true,false, Ordering::Release, Ordering::Relaxed).is_err() {
            continue; }

        self.bool_locked.store(false,Ordering::SeqCst);

        unsafe {
            let queueref: &mut HashMap<usize,Waker> = &mut *self.task_queue.get();
            if let Some(wk) = queueref.values().next() {
                wk.clone().wake(); }
        }
        self.can_queue.store(true,Ordering::Relaxed);
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
    wk_key : AtomicUsize,
}

impl<'a, T> FutureLock<'a, T> {
    /// trivial ctor
    fn new(_source: &'a Mutex<T>) -> FutureLock<'a, T> {
        FutureLock {
            source: _source,
            context_in_mtx: AtomicBool::new(false),
            wk_key : AtomicUsize::new(usize::MAX)
        }
    }
}

impl<'a, T> Future for FutureLock<'a, T> {
    type Output = LockGuard<'a, T>;

    /// Try to lock mutex
    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // on first poll, add the waker to PseudoMutex queue
        if !self.context_in_mtx.load(Ordering::Acquire) {
            self.wk_key.store(self.source.queue_waker(cx.waker().clone()).unwrap(), Ordering::SeqCst);
            self.context_in_mtx.store(true, Ordering::Release);
            Poll::Pending
        } else {
            //check if locked by attempting to lock. if success return Pseudoguard
            match self.source.bool_locked.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed) {
                Ok(_) => {
                    unsafe {
                        let queueref: &mut HashMap<usize,Waker> = &mut *self.source.task_queue.get();
                        let pair: Option<Waker> = queueref.remove(&self.wk_key.load(Ordering::SeqCst));
                        if pair.is_none() { panic!() }
                        self.source.is_async_locked.store(true, Ordering::Relaxed);
                        Poll::Ready(LockGuard::new(self.source))
                    }
                },
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
    /// Ctor, lock pseudo mutex given n argument. Private ctor.
    fn new(mtx: &'a Mutex<T>) -> LockGuard<'a, T> {
        LockGuard { lock: mtx}
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
