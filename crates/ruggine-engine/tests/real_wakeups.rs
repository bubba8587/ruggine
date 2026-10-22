// [[B1]] upstreamIsSpec: async node data woken from outside the graph, under a real executor.
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Wake, Waker};
use std::thread;
use std::time::Duration;

use ruggine::classic::{ClassicNode, Connection, Input, Node, Output, Socket};
use ruggine::future::LocalBoxFuture;
use ruggine::{GetSchemes, NodeBase, NodeEditor, keymap};
use ruggine_engine::{DataflowEngine, DataflowNode, Error, Inputs, Outputs};

/// Blocks the thread until `f` finishes, sleeping between wakeups.
fn block_on<F: std::future::Future>(f: F) -> F::Output {
    struct Unpark(thread::Thread);
    impl Wake for Unpark {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = Waker::from(Arc::new(Unpark(thread::current())));
    let mut f = std::pin::pin!(f);
    loop {
        match f.as_mut().poll(&mut Context::from_waker(&waker)) {
            Poll::Ready(v) => return v,
            Poll::Pending => thread::park_timeout(Duration::from_secs(5)),
        }
    }
}

/// A value another thread delivers after a delay.
struct Delivery {
    slot: Arc<Mutex<(Option<f64>, Option<Waker>)>>,
}

impl Delivery {
    fn after(ms: u64, value: f64) -> Self {
        let slot = Arc::new(Mutex::new((None, None::<Waker>)));
        let remote = slot.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(ms));
            let mut s = remote.lock().unwrap();
            s.0 = Some(value);
            if let Some(w) = s.1.take() {
                w.wake();
            }
        });
        Self { slot }
    }
}

impl std::future::Future for Delivery {
    type Output = f64;
    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<f64> {
        let mut s = self.slot.lock().unwrap();
        match s.0 {
            Some(v) => Poll::Ready(v),
            None => {
                s.1 = Some(cx.waker().clone());
                Poll::Pending
            }
        }
    }
}

struct Slow {
    node: Node,
    delay: u64,
    value: f64,
}

impl NodeBase for Slow {
    fn id(&self) -> &str {
        &self.node.id
    }
}

impl ClassicNode for Slow {
    type Socket = Socket;
    fn classic(&self) -> &Node {
        &self.node
    }
}

impl DataflowNode<f64> for Slow {
    fn data(&self, inputs: Inputs<f64>) -> LocalBoxFuture<'_, Result<Outputs<f64>, Error>> {
        let upstream: f64 = inputs.get("in").map_or(0.0, |v| v.iter().sum());
        let delivery = Delivery::after(self.delay, self.value);
        Box::pin(async move { Ok(keymap! { "out" => upstream + delivery.await }) })
    }
}

fn slow(delay: u64, value: f64) -> Rc<Slow> {
    let node = Node::new("slow");
    node.add_input(
        "in",
        Input::new(Socket::new("n")).with_multiple_connections(true),
    )
    .unwrap();
    node.add_output("out", Output::new(Socket::new("n")))
        .unwrap();
    Rc::new(Slow { node, delay, value })
}

#[test]
fn nodes_waiting_on_other_threads_finish_under_a_blocking_executor() {
    type S = GetSchemes<Slow, Connection>;
    let editor = NodeEditor::<S>::new();
    let engine = DataflowEngine::<S, f64>::new();
    editor.use_scope(&engine).unwrap();
    let a = slow(30, 1.0);
    let b = slow(10, 10.0);
    let sum = slow(5, 100.0);
    for n in [&a, &b, &sum] {
        block_on(editor.add_node(n.clone())).unwrap();
    }
    for n in [&a, &b] {
        let c = Connection::new(&**n, "out", &*sum, "in").unwrap();
        block_on(editor.add_connection(Rc::new(c))).unwrap();
    }
    let out = block_on(engine.fetch(sum.id())).unwrap();
    assert_eq!(out["out"], 111.0);
}
