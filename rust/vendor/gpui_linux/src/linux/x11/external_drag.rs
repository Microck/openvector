// OpenVector local addition: copy-only XDND source. The helper, not the UI window,
// owns the selection and grabs, so minimizing the UI cannot end the gesture.
use super::{X11Client, X11ClientState, XcbAtoms};
use crate::linux::x11::window::Callbacks;
use anyhow::{Result, anyhow, bail};
use calloop::{RegistrationToken, timer::{Timer, TimeoutAction}};
use gpui::{ExternalDragOutcome, ExternalDragPayload, FileDropEvent, PlatformInput};
use std::{cell::RefCell, collections::HashMap, rc::{Rc, Weak}, time::Duration};
use x11rb::{
    connection::Connection,
    cookie::Cookie,
    errors::ReplyError,
    protocol::{Event, xinput::{self, ConnectionExt as _}, xproto::{self, ConnectionExt as _}},
    wrapper::ConnectionExt as _,
    xcb_ffi::XCBConnection,
};

const START_TIMEOUT: Duration = Duration::from_secs(2);
const FINISH_TIMEOUT: Duration = Duration::from_secs(10);
const CHUNK_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Target {
    window: u32,
    recipient: u32,
    version: u32,
}

struct Transfer {
    offset: usize,
    previous_mask: xproto::EventMask,
}

pub(super) struct Source {
    pub(super) origin: u32,
    helper: u32,
    /// XI2 master pointer whose implicit grab on the origin the helper takes over.
    pointer_device: xinput::DeviceId,
    connection: Rc<XCBConnection>,
    atoms: XcbAtoms,
    callbacks: Weak<RefCell<Callbacks>>,
    uris: Vec<u8>,
    timestamp: u32,
    drop_time: u32,
    started: bool,
    pointer_grabbed: bool,
    keyboard_grabbed: bool,
    target: Option<Target>,
    awaiting_status: bool,
    accepted: bool,
    pending_position: Option<(i16, i16, u32)>,
    released: bool,
    dropped: bool,
    timeout: Option<RegistrationToken>,
    transfers: HashMap<(u32, u32), Transfer>,
}

impl X11Client {
    pub(crate) fn can_start_external_drag(&self) -> bool {
        self.0.borrow().external_drag.is_none()
    }

    pub(crate) fn start_external_drag(
        &self,
        origin: u32,
        callbacks: Weak<RefCell<Callbacks>>,
        payload: &ExternalDragPayload,
    ) -> bool {
        let mut state = self.0.borrow_mut();
        if state.external_drag.is_some() || !state.windows.contains_key(&origin) {
            return false;
        }
        // GPUI starts a native drag while dispatching a motion, so that motion's master holds the implicit grab.
        let Some(pointer_device) = state.last_motion_device else {
            log::warn!("XDND source rejected: no XI2 pointer motion preceded the drag");
            return false;
        };
        let uris = match uri_list(payload) {
            Ok(uris) => uris,
            Err(error) => { log::warn!("XDND source rejected: {error:#}"); return false; }
        };
        match Source::new(&mut state, origin, pointer_device, callbacks, uris) {
            Ok(source) => { state.external_drag = Some(source); true }
            Err(error) => { log::warn!("XDND source startup rejected: {error:#}"); false }
        }
    }

    pub(super) fn handle_external_drag_event(&self, event: &Event) -> bool {
        let mut state = self.0.borrow_mut();
        let Some(mut source) = state.external_drag.take() else { return false; };
        let result = source.handle(&mut state, event);
        match result {
            Ok((handled, None)) => { state.external_drag = Some(source); handled }
            Ok((handled, Some(outcome))) => { source.finish(&mut state, outcome); handled }
            Err(error) => {
                source.finish(&mut state, ExternalDragOutcome::Failed(format!("XDND: {error:#}")));
                true
            }
        }
    }
}

impl X11ClientState {
    pub(super) fn end_external_drag(&mut self, outcome: ExternalDragOutcome) {
        if let Some(source) = self.external_drag.take() {
            source.finish(self, outcome);
        }
    }
}

impl Source {
    fn new(
        state: &mut X11ClientState,
        origin: u32,
        pointer_device: xinput::DeviceId,
        callbacks: Weak<RefCell<Callbacks>>,
        uris: Vec<u8>,
    ) -> Result<Self> {
        let connection = state.xcb_connection.clone();
        let helper = connection.generate_id()?;
        let root = connection.setup().roots[state.x_root_index].root;
        connection.create_window(
            0, helper, root, -1, -1, 1, 1, 0, xproto::WindowClass::INPUT_ONLY, 0,
            &xproto::CreateWindowAux::new().override_redirect(1).event_mask(
                xproto::EventMask::PROPERTY_CHANGE | xproto::EventMask::STRUCTURE_NOTIFY),
        )?.check()?;
        let mut source = Self {
            origin, helper, pointer_device, connection, atoms: state.atoms.clone(), callbacks, uris,
            timestamp: 0, drop_time: 0, started: false, pointer_grabbed: false, keyboard_grabbed: false,
            target: None, awaiting_status: false, accepted: false, pending_position: None,
            released: false, dropped: false, timeout: None, transfers: HashMap::new(),
        };
        source.connection.map_window(helper)?.check()?;
        // PropertyNotify supplies a server timestamp and defers startup past the App borrow.
        source.connection.change_property32(xproto::PropMode::REPLACE, helper,
            source.atoms.XdndActionList, xproto::AtomEnum::ATOM,
            &[source.atoms.XdndActionCopy])?.check()?;
        source.arm_timeout(state, START_TIMEOUT)?;
        source.connection.flush()?;
        Ok(source)
    }

    fn arm_timeout(&mut self, state: &mut X11ClientState, duration: Duration) -> Result<()> {
        if let Some(token) = self.timeout.take() { state.loop_handle.remove(token); }
        let client = state.this.clone();
        let helper = self.helper;
        self.timeout = Some(state.loop_handle.insert_source(Timer::from_duration(duration), move |_, _, _| {
            if let Some(client) = client.get_client() {
                let mut state = client.0.borrow_mut();
                if let Some(mut source) = state.external_drag.take_if(|source| source.helper == helper) {
                    // The loop removes the firing timer; finish must not remove it twice.
                    source.timeout = None;
                    source.finish(&mut state, ExternalDragOutcome::Failed(
                        "XDND receiver/startup did not complete before its deadline".into()));
                }
            }
            TimeoutAction::Drop
        }).map_err(|error| anyhow!("Cannot register XDND deadline: {error:?}"))?);
        Ok(())
    }

    fn notify(&self, state: &X11ClientState, event: FileDropEvent) {
        let callbacks = self.callbacks.clone();
        // Idle callbacks run in FIFO order with no client/window/App borrows held.
        // Weak callbacks do not retain the UI or its renderer during a native session.
        state.loop_handle.insert_idle(move |_| {
            let Some(callbacks) = callbacks.upgrade() else { return; };
            let callback = callbacks.borrow_mut().input.take();
            if let Some(mut callback) = callback {
                callback(PlatformInput::FileDrop(event));
                callbacks.borrow_mut().input = Some(callback);
            }
        });
    }

    fn begin(&mut self, state: &mut X11ClientState, timestamp: u32) -> Result<()> {
        self.timestamp = timestamp;
        // The keyboard has no implicit grab to take over. Grabbing it before the pointer means a
        // foreign keyboard grab fails the drag while the origin still owns the gesture.
        let grab = self.connection.grab_keyboard(false, self.helper, timestamp,
            xproto::GrabMode::ASYNC, xproto::GrabMode::ASYNC)?.reply()?;
        if grab.status != xproto::GrabStatus::SUCCESS { bail!("Keyboard grab rejected: {:?}", grab.status); }
        self.keyboard_grabbed = true;
        // GPUI selects XI2 button events, so the initiating press gave the origin an XI2 implicit
        // grab, and the server answers a core GrabPointer over a grab of another type with
        // AlreadyGrabbed even for that grab's owner (dix GrabDevice). An XI2 grab of the same
        // master by the same client turns the implicit grab into an active one on the helper,
        // which neither the release nor unmapping the origin ends. Foreign grabs still refuse it.
        let grab = self.connection.xinput_xi_grab_device(self.helper, timestamp, x11rb::NONE, self.pointer_device,
            xproto::GrabMode::ASYNC, xproto::GrabMode::ASYNC, xinput::GrabOwner::NO_OWNER,
            &[u32::from(xinput::XIEventMask::MOTION | xinput::XIEventMask::BUTTON_PRESS | xinput::XIEventMask::BUTTON_RELEASE)])?.reply()?;
        if grab.status != xproto::GrabStatus::SUCCESS { bail!("Pointer grab rejected: {:?}", grab.status); }
        self.pointer_grabbed = true;
        // Checked only once the helper owns the device: a release between an earlier check and
        // the grab would go to the origin and leave the helper waiting for one. The core state
        // also counts touch-emulated presses, which XIQueryPointer omits for XI 2.2+ clients.
        let root = self.connection.setup().roots[state.x_root_index].root;
        let pointer = self.connection.query_pointer(root)?.reply()?;
        if !pointer.mask.contains(xproto::KeyButMask::BUTTON1) {
            bail!("The initiating pointer button is no longer held");
        }
        self.connection.set_selection_owner(self.helper, self.atoms.XdndSelection, timestamp)?.check()?;
        if self.connection.get_selection_owner(self.atoms.XdndSelection)?.reply()?.owner != self.helper {
            bail!("Could not own XdndSelection");
        }
        if let Some(token) = self.timeout.take() { state.loop_handle.remove(token); }
        self.started = true;
        self.notify(state, FileDropEvent::Started);
        self.motion(state, pointer.root_x, pointer.root_y, timestamp)?;
        Ok(())
    }

    fn send(&self, target: Target, type_: u32, data: [u32; 5]) -> Result<()> {
        // Proxy delivery keeps the actual target in the ClientMessage's window field.
        let event = xproto::ClientMessageEvent::new(32, target.window, type_, data);
        self.connection.send_event(false, target.recipient, xproto::EventMask::NO_EVENT, event)?.check()?;
        self.connection.flush()?;
        Ok(())
    }

    /// XdndLeave is advisory and never checked: a receiver that vanished while the pointer
    /// crossed it must neither cost a round trip nor send its BadWindow to the main event loop.
    fn leave(&self, target: Target) {
        let event = xproto::ClientMessageEvent::new(32, target.window, self.atoms.XdndLeave, [self.helper, 0, 0, 0, 0]);
        if let Ok(cookie) = self.connection.send_event(false, target.recipient, xproto::EventMask::NO_EVENT, event) {
            cookie.ignore_error();
        }
        let _ = self.connection.flush();
    }

    /// Queues a one-item property read so independent reads share a round trip.
    /// Resolve it with `decode32` and the same `type_`.
    fn read32(&self, window: u32, property: u32, type_: xproto::AtomEnum)
        -> Result<Cookie<'_, Rc<XCBConnection>, xproto::GetPropertyReply>> {
        Ok(self.connection.get_property(false, window, property, type_, 0, 1)?)
    }

    fn target_at_pointer(&self, state: &X11ClientState) -> Result<Option<Target>> {
        let mut window = self.connection.setup().roots[state.x_root_index].root;
        let mut target = None;
        // Bound traversal even under hostile/reparenting foreign window trees.
        for _ in 0..128 {
            // This app's own XdndAware windows would hand its export to the inbound XDND
            // handler as a new external file drop, so they are never targets of its source.
            if state.windows.contains_key(&window) { return Ok(None); }
            // Independent lookups share one round trip. Every reply is collected before any is
            // examined: a dropped cookie would deliver its BadWindow to the main event loop.
            let proxy = self.read32(window, self.atoms.XdndProxy, xproto::AtomEnum::WINDOW)?;
            // ANY type: upstream GPUI receivers tag XdndAware with an interned atom named
            // "XA_ATOM" instead of ATOM. `decode32` still requires one 32-bit item.
            let aware = self.read32(window, self.atoms.XdndAware, xproto::AtomEnum::ANY)?;
            let pointer = self.connection.query_pointer(window)?;
            let (proxy, aware, pointer) = (proxy.reply(), aware.reply(), pointer.reply());
            // A foreign window vanished during discovery.
            let Some(pointer) = unless_vanished(pointer)? else { return Ok(None); };
            let mut recipient = window;
            let mut version = decode32(aware, xproto::AtomEnum::ANY)?;
            if let Some(proxy) = decode32(proxy, xproto::AtomEnum::WINDOW)? {
                // Only a proxy pointing to itself is live; its XdndAware then applies.
                let check = self.read32(proxy, self.atoms.XdndProxy, xproto::AtomEnum::WINDOW)?;
                let proxy_aware = self.read32(proxy, self.atoms.XdndAware, xproto::AtomEnum::ANY)?;
                let (check, proxy_aware) = (check.reply(), proxy_aware.reply());
                if decode32(check, xproto::AtomEnum::WINDOW)? == Some(proxy) {
                    recipient = proxy;
                    version = decode32(proxy_aware, xproto::AtomEnum::ANY)?;
                }
            }
            if let Some(version) = version.filter(|version| *version >= 3) {
                target = Some(Target { window, recipient, version: version.min(5) });
            }
            if pointer.child == x11rb::NONE || pointer.child == self.helper { break; }
            window = pointer.child;
        }
        Ok(target)
    }

    fn motion(&mut self, state: &X11ClientState, x: i16, y: i16, time: u32) -> Result<()> {
        let target = self.target_at_pointer(state)?;
        if target != self.target {
            if let Some(old) = self.target {
                // Disappearance is normal while crossing windows. Do not abort the gesture.
                self.leave(old);
            }
            self.target = target;
            self.accepted = false;
            self.awaiting_status = false;
            self.pending_position = None;
            if let Some(target) = target {
                self.send(target, self.atoms.XdndEnter,
                    [self.helper, target.version << 24, self.atoms.TextUriList, 0, 0])?;
            }
        }
        if self.target.is_some() {
            if self.awaiting_status { self.pending_position = Some((x, y, time)); }
            else { self.position(x, y, time)?; }
        }
        Ok(())
    }

    fn position(&mut self, x: i16, y: i16, time: u32) -> Result<()> {
        let Some(target) = self.target else { return Ok(()); };
        self.send(target, self.atoms.XdndPosition,
            [self.helper, 0, ((x as u16 as u32) << 16) | y as u16 as u32, time, self.atoms.XdndActionCopy])?;
        self.awaiting_status = true;
        self.accepted = false;
        Ok(())
    }

    fn release_grabs(&mut self) {
        if self.pointer_grabbed {
            // A master removed mid-drag took its grab along; its BadDevice must not reach the main event loop.
            if let Ok(cookie) = self.connection.xinput_xi_ungrab_device(x11rb::CURRENT_TIME, self.pointer_device) {
                cookie.ignore_error();
            }
            self.pointer_grabbed = false;
        }
        if self.keyboard_grabbed {
            let _ = self.connection.ungrab_keyboard(x11rb::CURRENT_TIME);
            self.keyboard_grabbed = false;
        }
        let _ = self.connection.flush();
    }

    fn drop_if_ready(&mut self) -> Result<Option<ExternalDragOutcome>> {
        if !self.released || self.awaiting_status { return Ok(None); }
        let Some(target) = self.target.filter(|_| self.accepted) else {
            return Ok(Some(ExternalDragOutcome::Cancelled));
        };
        self.send(target, self.atoms.XdndDrop, [self.helper, 0, self.drop_time, 0, 0])?;
        self.dropped = true;
        Ok(None)
    }

    fn handle(&mut self, state: &mut X11ClientState, event: &Event) -> Result<(bool, Option<ExternalDragOutcome>)> {
        match event {
            Event::PropertyNotify(event) if event.window == self.helper && !self.started => {
                self.begin(state, event.time)?;
                return Ok((true, None));
            }
            // The XI2 grab reports pointer input to the helper as XI2 events only. Unlike core
            // MotionNotify, XI2 motion also reports scroll-only valuator changes; valuators 0 and 1
            // are x and y, the same test GPUI's own motion handler applies.
            Event::XinputMotion(event) if event.event == self.helper => {
                let moved = event.valuator_mask.first().is_some_and(|&mask| mask & 3 != 0);
                if self.started && !self.released && moved {
                    self.motion(state, root_pixel(event.root_x), root_pixel(event.root_y), event.time)?;
                }
            }
            Event::XinputButtonRelease(event) if event.event == self.helper => {
                if self.started && !self.released && event.detail == 1 {
                    self.motion(state, root_pixel(event.root_x), root_pixel(event.root_y), event.time)?;
                    self.drop_time = event.time;
                    self.released = true;
                    self.release_grabs();
                    self.arm_timeout(state, FINISH_TIMEOUT)?;
                    return Ok((true, self.drop_if_ready()?));
                }
            }
            Event::XinputButtonPress(event) if event.event == self.helper => {}
            Event::KeyPress(event) if event.event == self.helper => {
                let key_state = super::xkb_state_for_key_event(&state.xkb, event.state);
                if key_state.key_get_one_sym(xkbcommon::xkb::Keycode::new(event.detail as u32))
                    == xkbcommon::xkb::Keysym::Escape {
                    return Ok((true, Some(ExternalDragOutcome::Cancelled)));
                }
            }
            Event::KeyRelease(event) if event.event == self.helper => {}
            Event::ClientMessage(event) if event.window == self.helper => {
                if event.format != 32 { return Ok((true, None)); }
                let data = event.data.as_data32();
                let Some(target) = self.target.filter(|target| target.window == data[0]) else { return Ok((true, None)); };
                if event.type_ == self.atoms.XdndStatus && self.awaiting_status && !self.dropped {
                    self.awaiting_status = false;
                    self.accepted = data[1] & 1 != 0 && data[4] == self.atoms.XdndActionCopy;
                    if let Some((x, y, time)) = self.pending_position.take() { self.position(x, y, time)?; }
                    return Ok((true, self.drop_if_ready()?));
                }
                if event.type_ == self.atoms.XdndFinished && self.dropped {
                    // Before v5 Finished has no success/action fields. It acknowledges the
                    // previously accepted copy drop; v5 explicitly confirms both fields.
                    let accepted = target.version < 5 || (data[1] & 1 != 0 && data[2] == self.atoms.XdndActionCopy);
                    return Ok((true, Some(if accepted { ExternalDragOutcome::Dropped } else { ExternalDragOutcome::Cancelled })));
                }
            }
            Event::SelectionRequest(event) if event.owner == self.helper && event.selection == self.atoms.XdndSelection => {
                self.selection_request(event)?;
            }
            Event::SelectionClear(event) if event.owner == self.helper && event.selection == self.atoms.XdndSelection => {
                return Ok((true, Some(ExternalDragOutcome::Failed("XDND selection ownership was lost".into()))));
            }
            Event::DestroyNotify(event) if event.window == self.origin || event.window == self.helper => {
                return Ok((false, Some(ExternalDragOutcome::Cancelled)));
            }
            Event::DestroyNotify(event) if self.transfers.keys().any(|(window, _)| *window == event.window) => {
                // The requestor no longer exists, so there is no event mask to restore.
                self.transfers.retain(|(window, _), _| *window != event.window);
                return Ok((false, None));
            }
            Event::UnmapNotify(event) if event.window == self.helper => {
                return Ok((true, Some(ExternalDragOutcome::Failed("XDND gesture helper was unmapped".into()))));
            }
            Event::PropertyNotify(event) if event.state == xproto::Property::DELETE && self.transfers.contains_key(&(event.window, event.atom)) => {
                self.advance_transfer(event.window, event.atom)?;
            }
            _ => return Ok((false, None)),
        }
        Ok((true, None))
    }

    fn selection_request(&mut self, event: &xproto::SelectionRequestEvent) -> Result<()> {
        let property = if event.property == x11rb::NONE { event.target } else { event.property };
        let supported = event.target == self.atoms.TextUriList || event.target == self.atoms.TARGETS || event.target == self.atoms.TIMESTAMP;
        // A failed requestor must not poison the drag or unrelated clipboard ownership.
        let written = if supported { self.write_selection(event.requestor, property, event.target).is_ok() } else { false };
        let notify = xproto::SelectionNotifyEvent {
            response_type: xproto::SELECTION_NOTIFY_EVENT, sequence: 0, time: event.time,
            requestor: event.requestor, selection: event.selection, target: event.target,
            property: if written { property } else { x11rb::NONE },
        };
        // A vanished requestor's BadWindow is discarded without a round trip.
        self.connection.send_event(false, event.requestor, xproto::EventMask::NO_EVENT, notify)?.ignore_error();
        self.connection.flush()?;
        Ok(())
    }

    fn write_selection(&mut self, requestor: u32, property: u32, target: u32) -> Result<()> {
        if target == self.atoms.TARGETS {
            self.connection.change_property32(xproto::PropMode::REPLACE, requestor, property, xproto::AtomEnum::ATOM,
                &[self.atoms.TextUriList, self.atoms.TARGETS, self.atoms.TIMESTAMP])?.check()?;
        } else if target == self.atoms.TIMESTAMP {
            self.connection.change_property32(xproto::PropMode::REPLACE, requestor, property, xproto::AtomEnum::INTEGER, &[self.timestamp])?.check()?;
        } else if self.uris.len() <= CHUNK_BYTES {
            self.connection.change_property8(xproto::PropMode::REPLACE, requestor, property, self.atoms.TextUriList, &self.uris)?.check()?;
        } else {
            if self.transfers.contains_key(&(requestor, property)) { bail!("Selection transfer already pending"); }
            let current_mask = self.connection.get_window_attributes(requestor)?.reply()?.your_event_mask;
            let previous_mask = self.transfers.iter()
                .find(|((window, _), _)| *window == requestor)
                .map_or(current_mask, |(_, transfer)| transfer.previous_mask);
            self.connection.change_window_attributes(requestor, &xproto::ChangeWindowAttributesAux::new()
                .event_mask(current_mask | xproto::EventMask::PROPERTY_CHANGE | xproto::EventMask::STRUCTURE_NOTIFY))?.check()?;
            self.transfers.insert((requestor, property), Transfer { offset: 0, previous_mask });
            let result = self.connection.change_property32(xproto::PropMode::REPLACE, requestor, property, self.atoms.INCR,
                &[u32::try_from(self.uris.len())?])?.check();
            if result.is_err() { self.remove_transfer(requestor, property); }
            result?;
        }
        Ok(())
    }

    fn advance_transfer(&mut self, requestor: u32, property: u32) -> Result<()> {
        let transfer = self.transfers.get_mut(&(requestor, property)).unwrap();
        let end = (transfer.offset + CHUNK_BYTES).min(self.uris.len());
        let finished = transfer.offset == self.uris.len();
        let reply = self.connection.change_property8(xproto::PropMode::REPLACE, requestor, property,
            self.atoms.TextUriList, &self.uris[transfer.offset..end])?.check();
        transfer.offset = end;
        if finished || reply.is_err() { self.remove_transfer(requestor, property); }
        self.connection.flush()?;
        Ok(())
    }

    fn remove_transfer(&mut self, requestor: u32, property: u32) {
        if let Some(transfer) = self.transfers.remove(&(requestor, property)) {
            // Restore only after the last concurrent property transfer to this requestor. The
            // requestor may be gone, and its BadWindow must not reach the main event loop.
            if !self.transfers.keys().any(|(window, _)| *window == requestor)
                && let Ok(cookie) = self.connection.change_window_attributes(requestor,
                    &xproto::ChangeWindowAttributesAux::new().event_mask(transfer.previous_mask)) {
                cookie.ignore_error();
            }
        }
    }

    fn finish(mut self, state: &mut X11ClientState, outcome: ExternalDragOutcome) {
        if let Some(token) = self.timeout.take() { state.loop_handle.remove(token); }
        if !self.dropped && let Some(target) = self.target {
            self.leave(target);
        }
        self.notify(state, FileDropEvent::Ended { outcome });
        // Drop releases grabs, properties, selection ownership, and the helper before notification.
    }
}

impl Drop for Source {
    fn drop(&mut self) {
        self.release_grabs();
        while let Some(&(requestor, property)) = self.transfers.keys().next() {
            self.remove_transfer(requestor, property);
        }
        // Destroying our selection owner relinquishes only our selection, never a replacement owner.
        // If another client destroyed the helper (DestroyNotify ends the drag), ignore the BadWindow.
        if let Ok(cookie) = self.connection.destroy_window(self.helper) { cookie.ignore_error(); }
        let _ = self.connection.flush();
    }
}

/// Resolves a `read32` reply requested with `type_`; ANY accepts every type. Absent or
/// malformed values and vanished windows are `None`; connection failures stay fatal.
fn decode32(reply: Result<xproto::GetPropertyReply, ReplyError>, type_: xproto::AtomEnum) -> Result<Option<u32>> {
    let Some(reply) = unless_vanished(reply)? else { return Ok(None); };
    if (type_ != xproto::AtomEnum::ANY && reply.type_ != u32::from(type_)) || reply.format != 32 || reply.value_len != 1 {
        return Ok(None);
    }
    Ok(reply.value32().and_then(|mut values| values.next()))
}

/// Foreign windows can vanish between any two requests. Their X11 errors mean "absent";
/// connection errors abort the drag.
fn unless_vanished<T>(reply: Result<T, ReplyError>) -> Result<Option<T>> {
    match reply {
        Ok(reply) => Ok(Some(reply)),
        Err(ReplyError::X11Error(_)) => Ok(None),
        Err(ReplyError::ConnectionError(error)) => Err(error.into()),
    }
}

/// XI2 positions are 16.16 fixed point. XDND carries whole root pixels, the floor core events report.
fn root_pixel(value: xinput::Fp1616) -> i16 {
    (value >> 16) as i16
}

fn uri_list(payload: &ExternalDragPayload) -> Result<Vec<u8>> {
    let ExternalDragPayload::Files(paths) = payload;
    if paths.entries().is_empty() { bail!("No files were supplied"); }
    let mut uris = Vec::new();
    for (path, _) in paths.entries() {
        if !path.is_absolute() || !path.exists() { bail!("XDND requires existing absolute paths: {}", path.display()); }
        let uri = http_client::Url::from_file_path(path).map_err(|_| anyhow!("Invalid file path: {}", path.display()))?;
        uris.extend_from_slice(uri.as_str().as_bytes());
        uris.extend_from_slice(b"\r\n");
    }
    u32::try_from(uris.len()).map_err(|_| anyhow!("XDND URI list exceeds the X11 selection size limit"))?;
    Ok(uris)
}
