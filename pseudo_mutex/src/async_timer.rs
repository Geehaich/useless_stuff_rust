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
    pub fn sleep_duration(targ_d : Duration) -> AsyncTimeout
    {
        AsyncTimeout{target_time : Instant::now() + targ_d }
    }

    pub fn sleep_ms(millis : u64) -> AsyncTimeout
    {
        AsyncTimeout{target_time : Instant::now() + Duration::from_millis(millis)}
    }

    pub fn sleep_µs(micros : u64) -> AsyncTimeout
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
