#![feature(conservative_impl_trait)]
#![feature(generators)]
#![feature(immovable_types)]
#![feature(generator_trait)]

use std::marker::Move;
use std::ops::Generator;
use std::ops::GeneratorState as State;

pub struct NotReady(());

pub type Poll<R> = State<NotReady, R>;

pub trait Future: ?Move {
    type Return: ?Move;

    fn poll(&mut self) -> Poll<Self::Return>;
}

impl<'a, T: ?Move + Future> Future for &'a mut T {
    type Return = T::Return;

    fn poll(&mut self) -> Poll<Self::Return> {
        (*self).poll()
    }
}

pub struct AsFuture<T: ?Move>(T);

impl<T: Generator<Yield = NotReady, Return = R> + ?Move, R: ?Move> Future for AsFuture<T> {
    type Return = R;

    fn poll(&mut self) -> Poll<Self::Return> {
        self.0.resume()
    }
}

#[macro_export]
macro_rules! async {
    ($($b:tt)*) => ({
        AsFuture(static move || {
            // Force a generator by using `yield`
            if false { unsafe { yield ::std::mem::uninitialized() } };
            $($b)*
        })
    })
}

#[macro_export]
macro_rules! await {
    ($e:expr) => ({
        let mut future = $e;
        loop {
            match $crate::Future::poll(&mut future) {
                ::std::ops::GeneratorState::Complete(r) => break r,
                ::std::ops::GeneratorState::Yielded(not_ready) => yield not_ready,
            }
        }
    })
}

pub fn map<A, F, U>(future: A, f: F) -> impl Future<Return = U> 
where
    A: Future,
    F: FnOnce(A::Return) -> U,
{
    async! {
        let f = f;
        let r = await!(future);
        f(r)
    }
}

pub enum OneOf<A, B> {
    A(A),
    B(B),
}

impl<A: Future<Return = R>, B: Future<Return = R>, R: ?Move> Future for OneOf<A, B> {
    type Return = R;

    fn poll(&mut self) -> Poll<Self::Return> {
        match *self {
            OneOf::A(ref mut a) => a.poll(),
            OneOf::B(ref mut b) => b.poll(),
        }
    }
}

/// Returns the result of the first future to finish and the uncompleted future
/// This requires movable futures
pub fn select<A, B, R: ?Move>(mut a: A, mut b: B) -> impl Future<Return = (R, OneOf<A, B>)>
where
    A: Future<Return = R>,
    B: Future<Return = R>,
{
    async! {
        loop {
            match a.poll() {
                State::Complete(r) => return (r, OneOf::B(b)),
                State::Yielded(_) => (),
            }

            match b.poll() {
                State::Complete(r) => return (r, OneOf::A(a)),
                State::Yielded(y) => yield y,
            }
        }
    }
}

/// Returns the result of the first future to finish
pub fn race<A: ?Move, B: ?Move, R: ?Move>(mut a: A, mut b: B) -> impl Future<Return = R>
where
    A: Future<Return = R>,
    B: Future<Return = R>,
{
    async! {
        await!(select(&mut a, &mut b)).0
    }
}

/// Waits for two futures to complete
pub fn join<A: ?Move, B: ?Move, RA: ?Move, RB: ?Move>(mut a: A, mut b: B) -> impl Future<Return = (RA, RB)>
where
    A: Future<Return = RA>,
    B: Future<Return = RB>,
{
    async! {
        loop {
            match (a.poll(), b.poll()) {
                (State::Complete(ra), State::Complete(rb)) => return (ra, rb),
                (State::Yielded(y), _) | (_, State::Yielded(y)) => yield y,
            }
        }
    }
}