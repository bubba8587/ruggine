// [[B1]] upstreamIsSpec
//! Upstream's `ClassicPreset`: sockets, ports, controls, nodes and connections. Mapping notes:
//! `tree/specs/port.md` § Classic preset.

use std::any::Any;
use std::cell::{Cell, Ref, RefCell};
use std::ops::Deref;
use std::rc::Rc;

use crate::types::{ConnectionBase, GetSchemes, NodeBase, NodeId};
use crate::{Error, KeyMap, get_uid};

/// Upstream's `PortId`.
pub type PortId = String;

/// Upstream's `Socket`: what kind of value a port carries. Use your own type in its place for
/// richer sockets (`Node<MySocket>`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Socket {
    /// The socket's name.
    pub name: String,
}

impl Socket {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}

/// What inputs and outputs share.
pub struct Port<S = Socket> {
    /// A fresh [`get_uid`] on creation.
    pub id: PortId,
    socket: RefCell<Rc<S>>,
    label: RefCell<Option<String>>,
    multiple_connections: Cell<bool>,
    index: Cell<Option<f64>>,
}

impl<S> Port<S> {
    fn new(socket: Rc<S>, multiple_connections: bool) -> Self {
        Self {
            id: get_uid(),
            socket: RefCell::new(socket),
            label: RefCell::new(None),
            multiple_connections: Cell::new(multiple_connections),
            index: Cell::new(None),
        }
    }

    pub fn socket(&self) -> Rc<S> {
        self.socket.borrow().clone()
    }

    pub fn set_socket(&self, socket: impl Into<Rc<S>>) {
        *self.socket.borrow_mut() = socket.into();
    }

    pub fn label(&self) -> Option<String> {
        self.label.borrow().clone()
    }

    pub fn set_label(&self, label: Option<String>) {
        *self.label.borrow_mut() = label;
    }

    pub fn multiple_connections(&self) -> bool {
        self.multiple_connections.get()
    }

    pub fn set_multiple_connections(&self, multiple: bool) {
        self.multiple_connections.set(multiple);
    }

    /// Sort position; upstream's default is unset.
    pub fn index(&self) -> Option<f64> {
        self.index.get()
    }

    pub fn set_index(&self, index: Option<f64>) {
        self.index.set(index);
    }
}

/// Upstream's `Input`: a port values arrive on, with an optional control.
pub struct Input<S = Socket> {
    port: Port<S>,
    control: RefCell<Option<Rc<dyn AnyControl>>>,
    show_control: Cell<bool>,
}

impl<S> Input<S> {
    /// Takes one connection unless [`Input::with_multiple_connections`] says otherwise.
    pub fn new(socket: impl Into<Rc<S>>) -> Self {
        Self {
            port: Port::new(socket.into(), false),
            control: RefCell::new(None),
            show_control: Cell::new(true),
        }
    }

    pub fn with_label(self, label: impl Into<String>) -> Self {
        self.port.set_label(Some(label.into()));
        self
    }

    pub fn with_multiple_connections(self, multiple: bool) -> Self {
        self.port.set_multiple_connections(multiple);
        self
    }

    pub fn control(&self) -> Option<Rc<dyn AnyControl>> {
        self.control.borrow().clone()
    }

    /// Returns the control as now held.
    pub fn add_control<C: AnyControl>(&self, control: C) -> Result<Rc<C>, Error> {
        let control = Rc::new(control);
        self.add_control_rc(control.clone())?;
        Ok(control)
    }

    pub fn add_control_rc(&self, control: Rc<dyn AnyControl>) -> Result<(), Error> {
        let mut slot = self.control.borrow_mut();
        if slot.is_some() {
            return Err(Error::InputControlAlreadyAdded);
        }
        *slot = Some(control);
        Ok(())
    }

    pub fn remove_control(&self) {
        *self.control.borrow_mut() = None;
    }

    pub fn show_control(&self) -> bool {
        self.show_control.get()
    }

    pub fn set_show_control(&self, show: bool) {
        self.show_control.set(show);
    }
}

impl<S> Deref for Input<S> {
    type Target = Port<S>;

    fn deref(&self) -> &Port<S> {
        &self.port
    }
}

/// Upstream's `Output`: a port values leave by.
pub struct Output<S = Socket> {
    port: Port<S>,
}

impl<S> Output<S> {
    /// Takes many connections unless [`Output::with_multiple_connections`] says otherwise.
    pub fn new(socket: impl Into<Rc<S>>) -> Self {
        Self {
            port: Port::new(socket.into(), true),
        }
    }

    pub fn with_label(self, label: impl Into<String>) -> Self {
        self.port.set_label(Some(label.into()));
        self
    }

    pub fn with_multiple_connections(self, multiple: bool) -> Self {
        self.port.set_multiple_connections(multiple);
        self
    }
}

impl<S> Deref for Output<S> {
    type Target = Port<S>;

    fn deref(&self) -> &Port<S> {
        &self.port
    }
}

/// Upstream's base `Control` class.
#[derive(Debug)]
pub struct Control {
    /// A fresh [`get_uid`] on creation.
    pub id: String,
    index: Cell<Option<f64>>,
}

impl Control {
    pub fn new() -> Self {
        Self {
            id: get_uid(),
            index: Cell::new(None),
        }
    }

    pub fn index(&self) -> Option<f64> {
        self.index.get()
    }

    pub fn set_index(&self, index: Option<f64>) {
        self.index.set(index);
    }
}

impl Default for Control {
    fn default() -> Self {
        Self::new()
    }
}

/// Any control a node or input can hold: upstream's `Control` and its subclasses.
pub trait AnyControl: Any {
    fn control(&self) -> &Control;
}

impl AnyControl for Control {
    fn control(&self) -> &Control {
        self
    }
}

/// Recovers the concrete control type from a held control.
pub fn downcast_control<C: AnyControl>(control: Rc<dyn AnyControl>) -> Option<Rc<C>> {
    let control: Rc<dyn Any> = control;
    control.downcast::<C>().ok()
}

/// Upstream's `InputControl` type parameter: `'text'` or `'number'`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputControlKind {
    /// Holds text.
    Text,
    /// Holds a number.
    Number,
}

/// Called with the new value after every [`InputControl::set_value`].
pub type ChangeCallback<N> = Box<dyn Fn(Option<&N>)>;

/// Upstream's `InputControlOptions`.
pub struct InputControlOptions<N> {
    /// Whether the control is read-only (default `false`).
    pub readonly: bool,
    /// The starting value.
    pub initial: Option<N>,
    /// Called after every change.
    pub change: Option<ChangeCallback<N>>,
}

impl<N> Default for InputControlOptions<N> {
    fn default() -> Self {
        Self {
            readonly: false,
            initial: None,
            change: None,
        }
    }
}

/// Upstream's `InputControl`: a text control holds a `String`, a number control an `f64`.
pub struct InputControl<N> {
    control: Control,
    /// Text or number.
    pub kind: InputControlKind,
    value: RefCell<Option<N>>,
    readonly: Cell<bool>,
    change: Option<ChangeCallback<N>>,
}

impl InputControl<String> {
    pub fn text(options: InputControlOptions<String>) -> Self {
        Self::new(InputControlKind::Text, options)
    }
}

impl InputControl<f64> {
    pub fn number(options: InputControlOptions<f64>) -> Self {
        Self::new(InputControlKind::Number, options)
    }
}

impl<N> InputControl<N> {
    pub fn new(kind: InputControlKind, options: InputControlOptions<N>) -> Self {
        Self {
            control: Control::new(),
            kind,
            value: RefCell::new(options.initial),
            readonly: Cell::new(options.readonly),
            change: options.change,
        }
    }

    pub fn value(&self) -> Option<N>
    where
        N: Clone,
    {
        self.value.borrow().clone()
    }

    /// Sets the value, then calls the `change` option with it; the callback may set the value
    /// again.
    pub fn set_value(&self, value: Option<N>)
    where
        N: Clone,
    {
        *self.value.borrow_mut() = value.clone();
        if let Some(change) = &self.change {
            change(value.as_ref());
        }
    }

    pub fn readonly(&self) -> bool {
        self.readonly.get()
    }

    pub fn set_readonly(&self, readonly: bool) {
        self.readonly.set(readonly);
    }
}

impl<N: 'static> AnyControl for InputControl<N> {
    fn control(&self) -> &Control {
        &self.control
    }
}

impl<N> Deref for InputControl<N> {
    type Target = Control;

    fn deref(&self) -> &Control {
        &self.control
    }
}

/// Upstream's classic `Node`. Ports and controls change in place, through `&self`.
pub struct Node<S = Socket> {
    /// A fresh [`get_uid`] on creation; set your own before sharing the node, if you like.
    pub id: NodeId,
    label: RefCell<String>,
    inputs: RefCell<KeyMap<Rc<Input<S>>>>,
    outputs: RefCell<KeyMap<Rc<Output<S>>>>,
    controls: RefCell<KeyMap<Rc<dyn AnyControl>>>,
    selected: Cell<bool>,
    input_keys: RefCell<Option<Rc<[String]>>>,
    output_keys: RefCell<Option<Rc<[String]>>>,
}

impl<S> Node<S> {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            id: get_uid(),
            label: RefCell::new(label.into()),
            inputs: RefCell::new(KeyMap::new()),
            outputs: RefCell::new(KeyMap::new()),
            controls: RefCell::new(KeyMap::new()),
            selected: Cell::new(false),
            input_keys: RefCell::new(None),
            output_keys: RefCell::new(None),
        }
    }

    pub fn label(&self) -> String {
        self.label.borrow().clone()
    }

    pub fn set_label(&self, label: impl Into<String>) {
        *self.label.borrow_mut() = label.into();
    }

    pub fn selected(&self) -> bool {
        self.selected.get()
    }

    pub fn set_selected(&self, selected: bool) {
        self.selected.set(selected);
    }

    pub fn inputs(&self) -> Ref<'_, KeyMap<Rc<Input<S>>>> {
        self.inputs.borrow()
    }

    /// The input keys in order, shared until an input is added or removed.
    pub fn input_keys(&self) -> Rc<[String]> {
        cached_keys(&self.input_keys, || self.inputs.borrow().key_list())
    }

    pub fn input(&self, key: &str) -> Option<Rc<Input<S>>> {
        self.inputs.borrow().get(key).cloned()
    }

    pub fn has_input(&self, key: &str) -> bool {
        self.inputs.borrow().contains_key(key)
    }

    pub fn add_input(
        &self,
        key: impl Into<String>,
        input: impl Into<Rc<Input<S>>>,
    ) -> Result<(), Error> {
        let key = key.into();
        let mut inputs = self.inputs.borrow_mut();
        if inputs.contains_key(&key) {
            return Err(Error::InputAlreadyAdded(key));
        }
        inputs.insert(key, input.into());
        self.input_keys.take();
        Ok(())
    }

    pub fn remove_input(&self, key: &str) -> Option<Rc<Input<S>>> {
        self.input_keys.take();
        self.inputs.borrow_mut().remove(key)
    }

    pub fn outputs(&self) -> Ref<'_, KeyMap<Rc<Output<S>>>> {
        self.outputs.borrow()
    }

    /// The output keys in order, shared until an output is added or removed.
    pub fn output_keys(&self) -> Rc<[String]> {
        cached_keys(&self.output_keys, || self.outputs.borrow().key_list())
    }

    pub fn output(&self, key: &str) -> Option<Rc<Output<S>>> {
        self.outputs.borrow().get(key).cloned()
    }

    pub fn has_output(&self, key: &str) -> bool {
        self.outputs.borrow().contains_key(key)
    }

    pub fn add_output(
        &self,
        key: impl Into<String>,
        output: impl Into<Rc<Output<S>>>,
    ) -> Result<(), Error> {
        let key = key.into();
        let mut outputs = self.outputs.borrow_mut();
        if outputs.contains_key(&key) {
            return Err(Error::OutputAlreadyAdded(key));
        }
        outputs.insert(key, output.into());
        self.output_keys.take();
        Ok(())
    }

    pub fn remove_output(&self, key: &str) -> Option<Rc<Output<S>>> {
        self.output_keys.take();
        self.outputs.borrow_mut().remove(key)
    }

    pub fn controls(&self) -> Ref<'_, KeyMap<Rc<dyn AnyControl>>> {
        self.controls.borrow()
    }

    /// The control under `key`, if it is a `C`.
    pub fn control<C: AnyControl>(&self, key: &str) -> Option<Rc<C>> {
        let control = self.controls.borrow().get(key).cloned()?;
        downcast_control(control)
    }

    pub fn has_control(&self, key: &str) -> bool {
        self.controls.borrow().contains_key(key)
    }

    /// Returns the control as now held.
    pub fn add_control<C: AnyControl>(
        &self,
        key: impl Into<String>,
        control: C,
    ) -> Result<Rc<C>, Error> {
        let control = Rc::new(control);
        self.add_control_rc(key, control.clone())?;
        Ok(control)
    }

    pub fn add_control_rc(
        &self,
        key: impl Into<String>,
        control: Rc<dyn AnyControl>,
    ) -> Result<(), Error> {
        let key = key.into();
        let mut controls = self.controls.borrow_mut();
        if controls.contains_key(&key) {
            return Err(Error::ControlAlreadyAdded(key));
        }
        controls.insert(key, control);
        Ok(())
    }

    pub fn remove_control(&self, key: &str) -> Option<Rc<dyn AnyControl>> {
        self.controls.borrow_mut().remove(key)
    }
}

fn cached_keys(
    cache: &RefCell<Option<Rc<[String]>>>,
    list: impl FnOnce() -> Vec<String>,
) -> Rc<[String]> {
    cache
        .borrow_mut()
        .get_or_insert_with(|| list().into())
        .clone()
}

impl<S> NodeBase for Node<S> {
    fn id(&self) -> &str {
        &self.id
    }
}

/// A node built on the classic `Node`: what the engines and [`Connection::new`] read ports from.
/// A wrapper type returns its inner `Node`.
pub trait ClassicNode: NodeBase {
    /// The socket type of the node's ports.
    type Socket;

    fn classic(&self) -> &Node<Self::Socket>;
}

impl<S> ClassicNode for Node<S> {
    type Socket = S;

    fn classic(&self) -> &Node<S> {
        self
    }
}

/// Upstream's classic `Connection`: from one node's output to another node's input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Connection {
    /// A fresh [`get_uid`] on creation.
    pub id: String,
    /// The source node's id.
    pub source: NodeId,
    /// The target node's id.
    pub target: NodeId,
    /// The key of the source's output.
    pub source_output: String,
    /// The key of the target's input.
    pub target_input: String,
}

impl Connection {
    /// Fails unless `source` has the output and `target` has the input.
    pub fn new<A: ClassicNode + ?Sized, B: ClassicNode + ?Sized>(
        source: &A,
        source_output: impl Into<String>,
        target: &B,
        target_input: impl Into<String>,
    ) -> Result<Self, Error> {
        let source_output = source_output.into();
        let target_input = target_input.into();
        if !source.classic().has_output(&source_output) {
            return Err(Error::NoSourceOutput(source_output));
        }
        if !target.classic().has_input(&target_input) {
            return Err(Error::NoTargetInput(target_input));
        }
        Ok(Self {
            id: get_uid(),
            source: source.id().to_owned(),
            target: target.id().to_owned(),
            source_output,
            target_input,
        })
    }
}

impl ConnectionBase for Connection {
    fn id(&self) -> &str {
        &self.id
    }

    fn source(&self) -> &str {
        &self.source
    }

    fn target(&self) -> &str {
        &self.target
    }
}

/// A connection between named ports: what the engines follow.
pub trait ClassicConnection: ConnectionBase {
    fn source_output(&self) -> &str;
    fn target_input(&self) -> &str;
}

impl ClassicConnection for Connection {
    fn source_output(&self) -> &str {
        &self.source_output
    }

    fn target_input(&self) -> &str {
        &self.target_input
    }
}

/// rete-engine's `ClassicScheme`: classic nodes and connections.
pub type ClassicScheme = GetSchemes<Node, Connection>;
