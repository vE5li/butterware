use core::pin::Pin;
use core::task::{Context, Poll};

use futures::Future;

#[derive(Debug, Clone)]
pub enum Either3<A, B, C> {
    First(A),
    Second(B),
    Third(C),
}

/// Same as [`select`], but with more futures.
pub fn select3<A, B, C>(a: A, b: B, c: C) -> Select3<A, B, C>
where
    A: Future,
    B: Future,
    C: Future,
{
    Select3 { a, b, c }
}

/// Future for the [`select3`] function.
#[derive(Debug)]
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct Select3<A, B, C> {
    a: A,
    b: B,
    c: C,
}

impl<A, B, C> Future for Select3<A, B, C>
where
    A: Future,
    B: Future,
    C: Future,
{
    type Output = Either3<A::Output, B::Output, C::Output>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let a = unsafe { Pin::new_unchecked(&mut this.a) };
        let b = unsafe { Pin::new_unchecked(&mut this.b) };
        let c = unsafe { Pin::new_unchecked(&mut this.c) };

        if let Poll::Ready(x) = a.poll(cx) {
            return Poll::Ready(Either3::First(x));
        }

        if let Poll::Ready(x) = b.poll(cx) {
            return Poll::Ready(Either3::Second(x));
        }

        if let Poll::Ready(x) = c.poll(cx) {
            return Poll::Ready(Either3::Third(x));
        }

        Poll::Pending
    }
}
