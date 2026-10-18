#![allow(non_snake_case)] // crate name follows the VkLayer_* convention

mod loader;
mod device;
mod instance;
mod pnext;

use ash::vk;
use loader::*;
use std::collections::HashMap;
use std::sync::{Mutex, Once};

use instance::*;
use device::*;
use pnext::*;

// Logging

const LOG_ENV: &str = "VK_LAYER_EXAMPLE_RUST_LOG";

fn init_logging() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let _ = env_logger::Builder::new()
            .filter_level(log::LevelFilter::Warn)
            .parse_env(env_logger::Env::new().filter(LOG_ENV))
            .format_timestamp_millis()
            .try_init();
    });
}

// Layer

type LockedMap< K, T> = Mutex<HashMap<K, T>>;


macro_rules! pfn {
    ($f:expr) => {
        Some(std::mem::transmute::<*const (), unsafe extern "system" fn()>($f as *const ()))
    };
}
use pfn;

#[unsafe(no_mangle)]
pub unsafe extern "system" fn vkNegotiateLoaderLayerInterfaceVersion(
    p_version_struct: *mut VkNegotiateLayerInterface,
) -> vk::Result {
    init_logging();
    let interface = unsafe {
        let Some(s) = p_version_struct.as_mut() else {
            log::error!("vkNegotiateLoaderLayerInterfaceVersion called with a null struct");
            return vk::Result::ERROR_INITIALIZATION_FAILED;
        };
        s
    };
    if interface.s_type != VkNegotiateLayerStructType::LAYER_NEGOTIATE_INTERFACE_STRUCT {
        log::error!("unexpected negotiate struct type {:?}", interface.s_type);
        return vk::Result::ERROR_INITIALIZATION_FAILED;
    }
    if interface.loader_layer_interface_version < 2 {
        log::error!("loader only supports layer interface v{}; need v2", interface.loader_layer_interface_version);
        return vk::Result::ERROR_INITIALIZATION_FAILED;
    }
    interface.loader_layer_interface_version = CURRENT_LOADER_LAYER_INTERFACE_VERSION;
    interface.pfn_get_instance_proc_addr = Some(get_instance_proc_addr);
    interface.pfn_get_device_proc_addr = Some(get_device_proc_addr);
    interface.pfn_get_physical_device_proc_addr = None;
    log::info!("negotiated loader/layer interface v{CURRENT_LOADER_LAYER_INTERFACE_VERSION}");
    vk::Result::SUCCESS
}
