use std::ffi::{c_char, CStr, c_void};
use std::sync::OnceLock;
use ash::vk;
use crate::{find_in_p_next_chain, LockedMap};
use crate::instance::{instances, physical_devices};
use crate::loader::{VkLayerDeviceCreateInfo, VkLayerFunction};
use crate::pnext::clone_pnext;

pub struct DeviceData {
    device: ash::Device,
    get_proc_addr: vk::PFN_vkGetDeviceProcAddr,
}

pub fn devices() -> &'static LockedMap<vk::Device, DeviceData> {
    static M: OnceLock<LockedMap<vk::Device, DeviceData>> = OnceLock::new();
    M.get_or_init(Default::default)
}

pub unsafe fn device_proc_by_name(name: &[u8]) -> vk::PFN_vkVoidFunction {
    unsafe {
        match name {
            b"vkGetDeviceProcAddr" => crate::pfn!(get_device_proc_addr as vk::PFN_vkGetDeviceProcAddr),
            b"vkDestroyDevice" => crate::pfn!(destroy_device as vk::PFN_vkDestroyDevice),
            _ => None,
        }
    }
}

pub unsafe extern "system" fn get_device_proc_addr(
    device_handle: vk::Device,
    p_name: *const c_char,
) -> vk::PFN_vkVoidFunction {
    let proc = unsafe {
        let name = CStr::from_ptr(p_name).to_bytes();
        device_proc_by_name(name)
    };

    unsafe {
        if let Some(f) = proc {
            return Some(f);
        }
        let devices_guard_opt = devices().lock();
        if devices_guard_opt.is_err() {
            log::error!("Failed to lock instances");
            return None;
        }
        devices_guard_opt.unwrap().get(&device_handle)
            .and_then(|d| (d.get_proc_addr)(device_handle, p_name))
    }
}

pub unsafe extern "system" fn create_device(
    physical_device: vk::PhysicalDevice,
    p_create_info: *const vk::DeviceCreateInfo,
    p_allocator: *const vk::AllocationCallbacks,
    p_device: *mut vk::Device,
) -> vk::Result {
    let chain: *const VkLayerDeviceCreateInfo =
        unsafe { crate::find_in_p_next_chain((*p_create_info).p_next, vk::StructureType::LOADER_DEVICE_CREATE_INFO,
                             |info| info.function == VkLayerFunction::LAYER_LINK_INFO) };

    unsafe {
        if chain.is_null() || (*chain).pLayerInfo.is_null() {
            log::error!("no loader link info in the create-info pNext chain");
            return vk::Result::ERROR_INITIALIZATION_FAILED;
        }
    }

    // I hate it but Renderdoc seems to do something similar
    let chain_mut = unsafe { (chain as *mut VkLayerDeviceCreateInfo).as_mut().unwrap() };

    let link = unsafe { chain_mut.pLayerInfo.as_ref().unwrap() };
    let get_instance_proc_addr = link.pfnNextGetInstanceProcAddr;
    let get_device_proc_addr = link.pfnNextGetDeviceProcAddr;
    chain_mut.pLayerInfo = (*link).pNext;

    let Ok(physical_devices_guard) = physical_devices().lock() else {
        log::error!("Failed to lock physical devices");
        return vk::Result::ERROR_UNKNOWN;
    };
    let Some(physical_device_data) = physical_devices_guard.get(&physical_device) else {
        log::error!("Failed to find physical device in map");
        return vk::Result::ERROR_UNKNOWN;
    };

    let Ok(instances_guard) = instances().lock() else {
        log::error!("Could not lock instances");
        return vk::Result::ERROR_UNKNOWN;
    };
    let Some(instance_data) = instances_guard.get(&physical_device_data.instance_handle()) else {
        log::error!("Could not find instance");
        return vk::Result::ERROR_UNKNOWN;
    };
    let instance = instance_data.instance();

    let old_create_info = unsafe { p_create_info.as_ref().unwrap() };
    let mut new_create_info = old_create_info.clone();
    let supported_features = unsafe { instance.get_physical_device_features(physical_device) };

    let bump = bumpalo::Bump::new();
    let mut new_enabled_features: vk::PhysicalDeviceFeatures;
    let mut new_enabled_features_2: vk::PhysicalDeviceFeatures2 = vk::PhysicalDeviceFeatures2::default();
    let features_2: *const vk::PhysicalDeviceFeatures2 = unsafe { find_in_p_next_chain(old_create_info.p_next, vk::StructureType::PHYSICAL_DEVICE_FEATURES_2, |_| true) };
    if !features_2.is_null() || !old_create_info.p_enabled_features.is_null() {
        new_enabled_features = if !features_2.is_null() {
            unsafe { (*features_2).features.clone() }
        } else {
            unsafe { (*old_create_info.p_enabled_features).clone() }
        };
        // Hide our faked features from the lower layers & driver
        new_enabled_features.depth_bounds &= supported_features.depth_bounds;
        new_enabled_features.geometry_shader &= supported_features.geometry_shader;

        if !features_2.is_null() {
            new_enabled_features_2.features = new_enabled_features;
            new_create_info.p_enabled_features = &new_enabled_features as *const vk::PhysicalDeviceFeatures;
            let new_p_next = clone_pnext(old_create_info.p_next,
                                         &bump,
                                         |s| s != vk::StructureType::PHYSICAL_DEVICE_FEATURES_2
            );
            if let Some(new_p_next) = new_p_next {
                new_enabled_features_2.p_next = new_p_next as *mut vk::BaseOutStructure<'_> as *mut c_void;
            }
            new_create_info.p_next = &new_enabled_features_2 as *const vk::PhysicalDeviceFeatures2 as *const c_void;
        } else {
            new_create_info.p_enabled_features = &new_enabled_features as *const vk::PhysicalDeviceFeatures;
        }
    }

    let next_create = unsafe {
        let Some(next_create) = get_instance_proc_addr(instance.handle(), c"vkCreateDevice".as_ptr()) else {
            log::error!("next layer has no vkCreateDevice");
            return vk::Result::ERROR_INITIALIZATION_FAILED;
        };
        next_create
    };
    let res = unsafe {
        let next_create: vk::PFN_vkCreateDevice = std::mem::transmute(next_create);
        next_create(physical_device, &new_create_info as *const vk::DeviceCreateInfo, p_allocator, p_device)
    };
    if res != vk::Result::SUCCESS {
        log::warn!("next layer failed with {res:?}");
        return res;
    }

    let (device_handle, device) = unsafe {
        let device_handle = *p_device;
        let load = |n: &CStr| std::mem::transmute(get_device_proc_addr(device_handle, n.as_ptr()));
        let device = ash::Device::load_with(load, device_handle);
        (device_handle, device)
    };

    let data = DeviceData {
        device,
        get_proc_addr: get_device_proc_addr,
    };

    devices().lock().unwrap().insert(device_handle, data);
    log::info!("vkCreateDevice -> {device_handle:?}");
    vk::Result::SUCCESS
}

pub unsafe extern "system" fn destroy_device(
    device_handle: vk::Device,
    p_allocator: *const vk::AllocationCallbacks,
) {
    if device_handle == vk::Device::null() {
        return;
    }
    let Ok(mut devices_guard) = devices().lock() else {
        log::error!("Could not lock devices");
        return;
    };
    if let Some(device_data) = devices_guard.remove(&device_handle) {
        log::info!("vkDestroyDevice {device_handle:?}");
        unsafe { (device_data.device.fp_v1_0().destroy_device)(device_handle, p_allocator); }
    }
}
