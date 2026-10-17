#![allow(non_snake_case)] // crate name follows the VkLayer_* convention

mod loader;
mod device;
mod instance;

use ash::vk;
use loader::*;
use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::{Mutex, Once};

use instance::*;
use device::*;

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

pub unsafe fn find_in_p_next_chain<T>(mut p_next: *const c_void, s_type: vk::StructureType, callback: fn(&T) -> bool) -> *const T {
    while !p_next.is_null() {
        let base = unsafe { &*(p_next as *const vk::BaseInStructure) };
        if base.s_type == s_type {
            let typed_ptr = p_next as *const T;
            let typed_ref = unsafe { typed_ptr.as_ref().unwrap() };
            if callback(typed_ref) {
                return typed_ptr;
            }
        }
        p_next = base.p_next as *const c_void;
    }
    std::ptr::null()
}

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
    if interface.sType != VkNegotiateLayerStructType::LAYER_NEGOTIATE_INTERFACE_STRUCT {
        log::error!("unexpected negotiate struct type {:?}", interface.sType);
        return vk::Result::ERROR_INITIALIZATION_FAILED;
    }
    if interface.loaderLayerInterfaceVersion < 2 {
        log::error!("loader only supports layer interface v{}; need v2", interface.loaderLayerInterfaceVersion);
        return vk::Result::ERROR_INITIALIZATION_FAILED;
    }
    interface.loaderLayerInterfaceVersion = CURRENT_LOADER_LAYER_INTERFACE_VERSION;
    interface.pfnGetInstanceProcAddr = Some(get_instance_proc_addr);
    interface.pfnGetDeviceProcAddr = Some(get_device_proc_addr);
    interface.pfnGetPhysicalDeviceProcAddr = None;
    log::info!("negotiated loader/layer interface v{CURRENT_LOADER_LAYER_INTERFACE_VERSION}");
    vk::Result::SUCCESS
}
