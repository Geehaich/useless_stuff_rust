use std::ops::{Deref,DerefMut};
use std::sync::atomic::{AtomicBool,AtomicU16,Ordering};
use std::rc::Rc;
use futures::executor::block_on;
use std::thread::sleep;
use std::time::Duration;

static BASE_CLODOTEX_WAIT_NANOSECONDS :u32  = 500;



struct ClodoContainer<T>
{
    data : Rc::<T> ,
    manager : Rc::<ClodoTex<T>>
}

impl<T> Drop for ClodoContainer<T> {
    fn drop(&mut self)
    {
        self.manager.release();
    }
}

impl<T> Deref for ClodoContainer<T>
{
    type Target = T;
    fn deref(&self) -> &Self::Target 
    {
        &self.data
    }
}

impl<T> DerefMut for ClodoContainer<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.data
    }
}

struct ClodoTex<T>
{
    is_locked : AtomicBool,
    queue_size : AtomicU16,
    resource : T
}



unsafe impl<T> Send for ClodoTex<T> {}


impl<T> ClodoTex<T>
{
    
    fn new(resource :T) ->ClodoTex<T>
    {
        ClodoTex
        {
            is_locked : AtomicBool::new(false),
            queue_size : AtomicU16::new(0),
            resource : resource
        }
    }
    
    fn try_lock(&self) -> bool
    {
        match self.is_locked.compare_exchange(false,true, Ordering::Acquire,Ordering::Relaxed)
        {
            Ok(_x) => true,
            Err(_x) => 
            {
                sleep(self.get_wait_duration());
                false
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
    { Duration::new(0, self.queue_size.load(Ordering::Relaxed)as u32 *BASE_CLODOTEX_WAIT_NANOSECONDS)}
    
    async fn beg(&self) -> &mut T

}
    async fn async_main()
    {
        let a :i32 = 17 ;
        let mut tex = ClodoTex::<i32>::new(a);
        
        let clos_1 = async{
        let b : &mut i32 = (&tex).beg().await;
        *b += 2;
        println!("{}",b);
        tex.release();
        };

        let clos_2 = async {
        let c : &mut i32 = (&tex).beg().await;
        *c += 2;
        println!("{}",c);

        tex.release();
        };

        futures::join!(clos_1,clos_2);

 
    }

   fn main() 
    {
        block_on(async_main());
    }
