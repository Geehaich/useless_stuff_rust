use std::ops::{Deref,DerefMut};
use std::sync::atomic::{AtomicBool,AtomicU16,Ordering};
use std::cell::UnsafeCell;
use futures::executor::block_on;
use std::thread;
use std::sync::Arc;
use std::time::Duration;


static mut BASE_PSEUDOMUTEX_WAIT_NANOSECONDS :u32  = 500;


fn set_base_wait_ns(amount : u32) { unsafe{crate::BASE_PSEUDOMUTEX_WAIT_NANOSECONDS = amount;}}
struct PseudoGuard<'a, T>
{
    lock : &'a PseudoMutex<T>,
    //poisoned : PoisonError<T>
}

impl<'a, T> PseudoGuard<'a,T>
{    
    fn new(pseud : &'a PseudoMutex<T>) -> PseudoGuard<'a,T>
    {
        PseudoGuard { lock: pseud }//, poisoned: PoisonError::<T>::ok() }
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

struct PseudoMutex<T>
{
    is_locked : AtomicBool,
    queue_size : AtomicU16,
    data : UnsafeCell<T>
}



unsafe impl<T> Send for PseudoMutex<T> {}
unsafe impl<T> Sync for PseudoMutex<T> {}



impl<T> PseudoMutex<T>
{
    
    fn new(resource :T) ->PseudoMutex<T>
    {
        PseudoMutex::<T>
        {
            is_locked : AtomicBool::new(false),
            queue_size : AtomicU16::new(0),
            data: UnsafeCell::<T>::new(resource)
         }
        
        
    }
    
    async fn try_lock(&self) -> bool
    {
        match self.is_locked.compare_exchange(false,true,
             Ordering::Acquire,Ordering::Relaxed)
        {
            Ok(_x) => true,
            Err(_x) => 
            {
                false
            }
        }
    }

    async fn lock(&self,c : char) -> PseudoGuard<'_, T>
    {
        self.queue_size.fetch_add(1,Ordering::Relaxed);
        loop
        {
            if self.try_lock().await
            {
                return PseudoGuard::new(self);
            }
            else
            {
                println!("{}",c);
                thread::sleep(self.get_wait_duration())
            }
        }
    }

    
    fn release(&self) -> ()
    {
        self.queue_size.fetch_sub(1, Ordering::Relaxed);
        self.queue_size.fetch_min(0,Ordering::Relaxed);
        let _ = self.is_locked.compare_exchange(true,false, Ordering::Acquire,Ordering::Relaxed);
        
    }


    
    fn get_wait_duration(&self) -> Duration
    {
        unsafe
        {
         Duration::new(0,
         self.queue_size.load(Ordering::Relaxed)as u32 *crate::BASE_PSEUDOMUTEX_WAIT_NANOSECONDS)
    
           }
}



}

fn main()
{
    let a = 16;
    let testmut  = Arc::<PseudoMutex<i32>>::new(PseudoMutex::<i32>::new(a));

    let ref1 = Arc::clone(&testmut);
    let ref2 = Arc::clone(&testmut);

    let t_1 = thread::spawn( move ||  
        {
            let mut dats = block_on(ref1.lock('1'));
            thread::sleep(Duration::new(2,0));
            *dats +=2;
            println!("{}",*dats);
            ref1.release();
        }).join();

        let t_1 = thread::spawn(move|| 
            {
                let mut dats = block_on(ref2.lock('2'));
                thread::sleep(Duration::new(1,0));
                *dats +=3;
                println!("{}",*dats);
                ref2.release();
            }).join();
        

}

