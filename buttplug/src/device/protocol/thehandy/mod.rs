use super::{ButtplugDeviceResultFuture, ButtplugProtocol, ButtplugProtocolCommandHandler};
use crate::{
  core::{
    errors::{ButtplugError, ButtplugDeviceError},
    messages::{self, ButtplugDeviceCommandMessageUnion, DeviceMessageAttributesMap},
  },
  device::{
    protocol::{generic_command_manager::GenericCommandManager, ButtplugProtocolProperties},
    DeviceImpl,
    DeviceWriteCmd,
    DeviceReadCmd,
    Endpoint,
  },
};
use futures::future::{self, BoxFuture};
use std::sync::Arc;
use tokio::sync::Mutex;
use prost::Message;

mod protocomm {
  include!(concat!(env!("OUT_DIR"), "/protocomm.rs"));
}

mod handyplug {
  include!(concat!(env!("OUT_DIR"), "/handyplug.rs"));
}

#[derive(ButtplugProtocolProperties)]
pub struct TheHandy {
  name: String,
  message_attributes: DeviceMessageAttributesMap,
  manager: Arc<Mutex<GenericCommandManager>>,
  stop_commands: Vec<ButtplugDeviceCommandMessageUnion>,
}


impl ButtplugProtocol for TheHandy {
  fn new_protocol(
    name: &str,
    message_attributes: DeviceMessageAttributesMap,
  ) -> Box<dyn ButtplugProtocol>
  where
    Self: Sized,
  {
    let manager = GenericCommandManager::new(&message_attributes);

    Box::new(Self {
      name: name.to_owned(),
      message_attributes,
      stop_commands: manager.get_stop_commands(),
      manager: Arc::new(Mutex::new(manager)),
    })
  }

  fn initialize(device_impl: Arc<DeviceImpl>) -> BoxFuture<'static, Result<Option<String>, ButtplugError>>
  where
      Self: Sized, 
  {
    Box::pin(async move {
      // Ok, here we go. This is an overly-complex nightmare but apparently
      // "protocomm makes the firmware easier".
      //
      // This code is mostly my translation of the Handy Python POC. It leaves
      // out a lot of stuff that doesn't seem needed (ping messages, the whole
      // RequestServerInfo flow, etc...) If they ever change anything, I quit.
      //
      // If you are a sex toy manufacturer reading this code: Please, talk to me
      // before implementing your protocol. Buttplug is not made to be a
      // hardware/firmware protocol, and you will regret trying to make it such.
  
      // First we need to set up a session with The Handy. This will require
      // sending the "security initializer" to basically say we're sending
      // plaintext. Due to pb3 making everything optional, we have some Option<T>
      // wrappers here.
      let session_req = protocomm::SessionData {
        sec_ver: protocomm::SecSchemeVersion::SecScheme0 as i32,
        proto: Some(protocomm::session_data::Proto::Sec0(
          protocomm::Sec0Payload {
            msg: protocomm::Sec0MsgType::S0SessionCommand as i32,
            payload: Some(protocomm::sec0_payload::Payload::Sc(protocomm::S0SessionCmd {}))
          }
        ))
      };
  
      // We need to shove this at what we're calling the "firmware" endpoint but
      // is actually the "prov-session" characteristic. These names are stored
      // in characteristic descriptors, which isn't super common on sex toys
      // (with exceptions for things that have a lot of sensors, like the Lelo
      // F1s).
      //
      // I don't have to do characteristic descriptor lookups for the
      // other 140+ pieces of hardware this library supports so I'm damn well
      // not doing it now. YOLO'ing hardcoded values from the device config.
      //
      // If they ever change this, I quit (or will just update the device config).

      let mut sec_buf = vec![];
      session_req.encode(&mut sec_buf).unwrap();
      device_impl.write_value(DeviceWriteCmd::new(Endpoint::Firmware, sec_buf, false));
      let _ = device_impl.read_value(DeviceReadCmd::new(Endpoint::Firmware, 100, 500));

      // At this point, the "handyplug" protocol does actually have both
      // RequestServerInfo and Ping messages that it can use. However, having
      // removed these and still tried to run the system, it seems fine. I've
      // omitted those for the moment, and will readd the complexity once it
      // does not seem needless.
      //
      // We have no device name updates here, so just return None.
      Ok(None)
    })
  }
}

impl ButtplugProtocolCommandHandler for TheHandy {
  fn handle_linear_cmd(&self, device: Arc<DeviceImpl>, message: messages::LinearCmd) -> ButtplugDeviceResultFuture {
    // What is "How not to implement a command structure for your device that does one thing", Alex?

    // First make sure we only have one vector.
    //
    // TODO Use the command manager to check this.
    if message.vectors().len() != 1 {
      return Box::pin(future::ready(Err(ButtplugDeviceError::DeviceFeatureCountMismatch(1, message.vectors().len() as u32).into())));
    }

    let linear = handyplug::LinearCmd {
      // You know when message IDs are important? When you have a protocol that
      // handles multiple asynchronous commands. You know what doesn't handle
      // multiple asynchronous commands? The handyplug protocol.
      //
      // Do you know where you'd pack those? In the top level container, as
      // they should then be separate from the message context, in order to
      // allow multiple sorters. Do you know what doesn't need multiple
      // sorters? The handyplug protocol.
      //
      // Please do not cargo cult protocols.
      id: 2,
      // You know when multiple device indicies are important? WHEN YOU HAVE
      // MULTIPLE DEVICE CONNECTI... oh fuck it. I am so tired. I am going to
      // bed.
      device_index: 0,
      // AND I'M BACK AND WELL RESTED. You know when multiple axes are
      // important? When you have to support arbitrary devices with multiple
      // axes. You know what device doesn't have multiple axes? 
      //
      // Guess.
      //
      // I'll wait.
      //
      // The handy. It's the handy.
      vectors: vec!(
        handyplug::linear_cmd::Vector {
          index: 0,
          duration: message.vectors()[0].duration(),
          position: *message.vectors()[0].position()
        }
      )
    };
    let linear_payload = handyplug::Payload {
      messages: vec!(handyplug::Message {
        message: Some(handyplug::message::Message::LinearCmd(linear))
      })
    };
    let mut linear_buf = vec!();
    linear_payload.encode(&mut linear_buf).unwrap();
    Box::pin(async move {
      device.write_value(DeviceWriteCmd::new(Endpoint::Tx, linear_buf, true)).await?;
      Ok(messages::Ok::default().into())
    })
  }
}