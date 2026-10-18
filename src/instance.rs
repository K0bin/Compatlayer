use std::ffi::{c_char, CStr};
use std::sync::OnceLock;
use ash::vk;
use crate::{LockedMap, find_in_p_next_chain, pfn};
use crate::loader::{VkLayerInstanceCreateInfo, VkLayerFunction};

pub struct InstanceData {
    pub app_name: String,
    pub app_version: u32,
    pub engine_name: String,
    pub engine_version: u32,
    pub api_version: u32,

    instance: ash::Instance,
    get_proc_addr: vk::PFN_vkGetInstanceProcAddr,
}

impl InstanceData {
    #[inline(always)]
    pub fn instance(&self) -> &ash::Instance {
        &self.instance
    }

    #[inline(always)]
    pub fn get_proc_addr(&self) -> &vk::PFN_vkGetInstanceProcAddr {
        &self.get_proc_addr
    }
}

pub fn instances() -> &'static LockedMap<vk::Instance, InstanceData> {
    static M: OnceLock<LockedMap<vk::Instance, InstanceData>> = OnceLock::new();
    M.get_or_init(Default::default)
}


pub struct PhysicalDeviceData {
    instance_handle: vk::Instance,
}
impl PhysicalDeviceData {
    #[inline(always)]
    pub fn instance_handle(&self) -> vk::Instance {
        self.instance_handle
    }
}

pub fn physical_devices() -> &'static LockedMap<vk::PhysicalDevice, PhysicalDeviceData> {
    static M: OnceLock<LockedMap<vk::PhysicalDevice, PhysicalDeviceData>> = OnceLock::new();
    M.get_or_init(Default::default)
}

pub unsafe extern "system" fn get_instance_proc_addr(
    instance_handle: vk::Instance,
    p_name: *const c_char,
) -> vk::PFN_vkVoidFunction {
    let name_bytes = unsafe { CStr::from_ptr(p_name).to_bytes() };
    let proc = unsafe {
        match name_bytes {
            b"vkGetInstanceProcAddr" => pfn!(get_instance_proc_addr as vk::PFN_vkGetInstanceProcAddr),
            b"vkCreateInstance" => pfn!(create_instance as vk::PFN_vkCreateInstance),
            b"vkDestroyInstance" => pfn!(destroy_instance as vk::PFN_vkDestroyInstance),
            b"vkCreateDevice" => pfn!(crate::create_device as vk::PFN_vkCreateDevice),
            b"vkEnumeratePhysicalDevices" => pfn!(enumerate_physical_devices as vk::PFN_vkEnumeratePhysicalDevices),
            b"vkGetPhysicalDeviceFeatures" => pfn!(get_physical_device_features as vk::PFN_vkGetPhysicalDeviceFeatures),
            b"vkGetPhysicalDeviceFeatures2" => pfn!(get_physical_device_features2 as vk::PFN_vkGetPhysicalDeviceFeatures2),
            b"vkGetPhysicalDeviceFeatures2KHR" => pfn!(get_physical_device_features2 as vk::PFN_vkGetPhysicalDeviceFeatures2),
            _ => None,
        }
    };

    unsafe {
        if let Some(f) = proc.or_else(|| crate::device_proc_by_name(name_bytes)) {
            return Some(f);
        }
        if instance_handle == vk::Instance::null() {
            return None;
        }
        let Ok(instance_guard) = instances().lock() else {
            log::error!("Failed to lock instances");
            return None;
        };
        instance_guard.get(&instance_handle)
            .and_then(|d| d.get_proc_addr()(instance_handle, p_name))
    }
}

pub unsafe extern "system" fn create_instance(
    p_create_info: *const vk::InstanceCreateInfo,
    p_allocator: *const vk::AllocationCallbacks,
    p_instance: *mut vk::Instance,
) -> vk::Result {
    let chain: *const VkLayerInstanceCreateInfo =
        unsafe { find_in_p_next_chain((*p_create_info).p_next,
        |info| info.function == VkLayerFunction::LAYER_LINK_INFO) };

    unsafe {
        if chain.is_null() || (*chain).contents.p_layer_info.is_null() {
            log::error!("no loader link info in the create-info pNext chain");
            return vk::Result::ERROR_INITIALIZATION_FAILED;
        }
    }

    // I hate it but Renderdoc seems to do something similar
    let chain_mut = chain as *mut VkLayerInstanceCreateInfo;

    let link = unsafe { (*chain_mut).contents.p_layer_info };
    let get_proc_addr = unsafe { (*link).pfn_next_get_instance_proc_addr };
    // Advance the chain so the next layer sees its own link.
    unsafe { (*chain_mut).contents.p_layer_info = (*link).p_next; }

    let next_create = unsafe {
        let Some(next_create) = get_proc_addr(vk::Instance::null(), c"vkCreateInstance".as_ptr()) else {
            log::error!("next layer has no vkCreateInstance");
            return vk::Result::ERROR_INITIALIZATION_FAILED;
        };
        next_create
    };
    let next_create: vk::PFN_vkCreateInstance = unsafe { std::mem::transmute(next_create) };
    let res = unsafe { next_create(p_create_info, p_allocator, p_instance) };
    if res != vk::Result::SUCCESS {
        log::warn!("next layer failed with {res:?}");
        return res;
    }

    let (instance_handle, instance) = unsafe {
        let instance_handle = *p_instance;
        let instance = ash::Instance::load_with(|n| std::mem::transmute(get_proc_addr(instance_handle, n.as_ptr())), instance_handle);
        (instance_handle, instance)
    };

    let app_info = unsafe { (*p_create_info).p_application_info.as_ref() };
    let app_name = app_info
        .and_then(|app_info| unsafe { CStr::from_ptr(app_info.p_application_name) }
        .to_str().ok().map(|s| s.to_string())).unwrap_or(String::new());
    let app_version = app_info.map_or(0, |app_info| app_info.application_version);
    let engine_name = app_info
        .and_then(|app_info| unsafe { CStr::from_ptr(app_info.p_engine_name) }
            .to_str().ok().map(|s| s.to_string())).unwrap_or(String::new());
    let engine_version = app_info.map_or(0, |app_info| app_info.engine_version);
    let api_version = app_info.map_or(0, |app_info| app_info.api_version);

    let data = InstanceData { instance, get_proc_addr, app_name, app_version, engine_name, engine_version, api_version };
    instances()
        .lock()
        .unwrap()
        .insert(instance_handle, data);
    log::info!("vkCreateInstance -> {instance_handle:?}");
    vk::Result::SUCCESS
}

pub unsafe extern "system" fn enumerate_physical_devices(instance_handle: vk::Instance, count: *mut u32, data: *mut vk::PhysicalDevice) -> vk::Result {
    let Ok(instances_guard) = instances().lock() else {
        log::error!("Failed to lock instances");
        return vk::Result::ERROR_UNKNOWN;
    };
    let Some(instance) = instances_guard.get(&instance_handle) else {
        log::error!("Failed to find instance in map");
        return vk::Result::ERROR_UNKNOWN;
    };

    let physical_devices_res = unsafe { (instance.instance.fp_v1_0().enumerate_physical_devices)(instance.instance.handle(), count, data) };
    if physical_devices_res != vk::Result::SUCCESS || data.is_null() {
        return physical_devices_res;
    }
    let count_usize = unsafe { *count } as usize;

    let Ok(mut physical_devices_guard) = physical_devices().lock() else {
        log::error!("Failed to lock physical devices");
        return vk::Result::ERROR_UNKNOWN;
    };
    let physical_devices = unsafe { std::slice::from_raw_parts(data, count_usize)};
    for physical_device in physical_devices {
        let data = PhysicalDeviceData { instance_handle };
        physical_devices_guard.insert(*physical_device, data);
    }

    vk::Result::SUCCESS
}

pub unsafe extern "system" fn destroy_instance(
    instance_handle: vk::Instance,
    p_allocator: *const vk::AllocationCallbacks,
) {
    if instance_handle == vk::Instance::null() {
        return;
    }

    let Ok(mut physical_devices_guard) = physical_devices().lock() else {
        log::error!("Failed to lock physical_devices");
        return;
    };
    physical_devices_guard.retain(|_, data| data.instance_handle != instance_handle);
    std::mem::drop(physical_devices_guard);

    let Ok(mut instances_guard) = instances().lock() else {
        log::error!("Failed to lock instances");
        return;
    };
    if let Some(instance_data) = instances_guard.remove(&instance_handle) {
        log::info!("vkDestroyInstance {instance_handle:?}");
        unsafe { (instance_data.instance().fp_v1_0().destroy_instance)(instance_handle, p_allocator); }
    }
}

fn modify_base_features(instance_data: &InstanceData, features: &mut vk::PhysicalDeviceFeatures) {
    if instance_data.engine_name == "idTech" {
        log::info!("Faking depth bounds for idTech.");
        features.depth_bounds = vk::TRUE;
    }
    if instance_data.engine_name == "DXVK" {
        log::info!("Faking fillModeNonSolid for DXVK.");
        features.fill_mode_non_solid = vk::TRUE;

        if instance_data.app_version & 1 == 1 {
            log::info!("Faking geometry shaders for DXVK D3D9.");
            features.geometry_shader = vk::TRUE;
        }
    }
}

pub unsafe extern "system" fn get_physical_device_features(
    physical_device_handle: vk::PhysicalDevice,
    features: *mut vk::PhysicalDeviceFeatures
) {
    if features.is_null() {
        return;
    }
    let Ok(physical_devices_guard) = physical_devices().lock() else {
        log::error!("Could not lock physical devices");
        return;
    };
    let Some(physical_device_data) = physical_devices_guard.get(&physical_device_handle) else {
        log::error!("Could not find physical device");
        return;
    };

    let Ok(instances_guard) = instances().lock() else {
        log::error!("Could not lock instances");
        return;
    };
    let Some(instance_data) = instances_guard.get(&physical_device_data.instance_handle) else {
        log::error!("Could not find instance");
        return;
    };

    unsafe { (instance_data.instance.fp_v1_0().get_physical_device_features)(physical_device_handle, features); }
    let features_mut = unsafe { &mut (*features) };
    modify_base_features(instance_data, features_mut);
}

pub unsafe extern "system" fn get_physical_device_features2(
    physical_device_handle: vk::PhysicalDevice,
    features: *mut vk::PhysicalDeviceFeatures2
) {
    if features.is_null() {
        return;
    }
    let Ok(physical_devices_guard) = physical_devices().lock() else {
        log::error!("Could not lock physical devices");
        return;
    };
    let Some(physical_device_data) = physical_devices_guard.get(&physical_device_handle) else {
        log::error!("Could not find physical device");
        return;
    };

    let Ok(instances_guard) = instances().lock() else {
        log::error!("Could not lock instances");
        return;
    };
    let Some(instance_data) = instances_guard.get(&physical_device_data.instance_handle) else {
        log::error!("Could not find instance");
        return;
    };

    unsafe { (instance_data.instance.fp_v1_1().get_physical_device_features2)(physical_device_handle, features); }
    let features_mut = unsafe { &mut (*features).features };
    modify_base_features(instance_data, features_mut);
}
