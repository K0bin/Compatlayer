use std::ffi::{c_char, CStr, c_void};
use std::sync::OnceLock;
use ash::vk;
use ash::vk::Handle;
use crate::{clone_p_next_chain, find_in_p_next_chain, LockedMap};
use crate::instance::{instances, physical_devices};
use crate::loader::{VkLayerDeviceCreateInfo, VkLayerFunction};

pub struct DeviceData {
    device: ash::Device,
    extended_dynamic_state: Option<ash::ext::extended_dynamic_state::Device>,
    get_proc_addr: vk::PFN_vkGetDeviceProcAddr,
    supported_features: vk::PhysicalDeviceFeatures,
    enabled_extensions: Vec<String>,
}

pub fn devices() -> &'static LockedMap<vk::Device, DeviceData> {
    static M: OnceLock<LockedMap<vk::Device, DeviceData>> = OnceLock::new();
    M.get_or_init(Default::default)
}

pub struct CommandBufferData {
    device_handle: vk::Device,
    cmd_pool: vk::CommandPool,
}

pub fn cmd_buffers() -> &'static LockedMap<vk::CommandBuffer, crate::CommandBufferData> {
    static M: OnceLock<LockedMap<vk::CommandBuffer, crate::CommandBufferData>> = OnceLock::new();
    M.get_or_init(Default::default)
}


pub unsafe fn device_proc_by_name(name: &[u8]) -> vk::PFN_vkVoidFunction {
    unsafe {
        match name {
            b"vkGetDeviceProcAddr" => crate::pfn!(get_device_proc_addr as vk::PFN_vkGetDeviceProcAddr),
            b"vkDestroyDevice" => crate::pfn!(destroy_device as vk::PFN_vkDestroyDevice),
            b"vkCmdSetDepthBoundsTestEnable" => crate::pfn!(cmd_set_depth_bounds_test_enable::<false> as vk::PFN_vkCmdSetDepthBoundsTestEnable),
            b"vkCmdSetDepthBoundsTestEnableEXT" => crate::pfn!(cmd_set_depth_bounds_test_enable::<true> as vk::PFN_vkCmdSetDepthBoundsTestEnable),
            b"vkDestroyCommandPool" => crate::pfn!(destroy_command_pool as vk::PFN_vkDestroyCommandPool),
            b"vkResetCommandPool" => crate::pfn!(reset_command_pool as vk::PFN_vkResetCommandPool),
            b"vkAllocateCommandBuffers" => crate::pfn!(allocate_command_buffer as vk::PFN_vkAllocateCommandBuffers),
            b"vkFreeCommandBuffers" => crate::pfn!(free_command_buffers as vk::PFN_vkFreeCommandBuffers),
            b"vkCreateGraphicsPipelines" => crate::pfn!(create_graphics_pipelines as vk::PFN_vkCreateGraphicsPipelines),
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
        unsafe { crate::find_in_p_next_chain((*p_create_info).p_next,
                             |info| info.function == VkLayerFunction::LAYER_LINK_INFO) };

    unsafe {
        if chain.is_null() || (*chain).p_layer_info.is_null() {
            log::error!("no loader link info in the create-info pNext chain");
            return vk::Result::ERROR_INITIALIZATION_FAILED;
        }
    }

    // I hate it but Renderdoc seems to do something similar
    let chain_mut = chain as *mut VkLayerDeviceCreateInfo;

    let link = unsafe { (*chain_mut).p_layer_info };
    let get_instance_proc_addr = unsafe { (*link).pfn_next_get_instance_proc_addr };
    let get_device_proc_addr = unsafe { (*link).pfn_next_get_device_proc_addr };
    unsafe { (*chain_mut).p_layer_info = (*link).p_next; }

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
    let features_2: *const vk::PhysicalDeviceFeatures2 = unsafe { find_in_p_next_chain(old_create_info.p_next, |_| true) };
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
            let new_p_next = unsafe {
                clone_p_next_chain(old_create_info.p_next, &bump,
                            |s| s != vk::StructureType::PHYSICAL_DEVICE_FEATURES_2
                )
            };
            if let Some(new_p_next) = new_p_next {
                new_enabled_features_2.p_next = new_p_next as *mut vk::BaseOutStructure<'_> as *mut c_void;
            }
            new_create_info.p_next = &new_enabled_features_2 as *const vk::PhysicalDeviceFeatures2 as *const c_void;
        } else {
            new_create_info.p_enabled_features = &new_enabled_features as *const vk::PhysicalDeviceFeatures;
        }
    }

    let enabled_extensions = unsafe { std::slice::from_raw_parts(old_create_info.pp_enabled_extension_names, old_create_info.enabled_extension_count as usize) };
    let enabled_extensions_strs: Vec<String> = unsafe { enabled_extensions.iter().map(|s| std::ffi::CStr::from_ptr(*s).to_string_lossy().to_string()) }.collect();

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

    let extended_dynamic_state = if enabled_extensions_strs.iter().any(|s| s == "VK_EXT_extended_dynamic_state") {
        Some(ash::ext::extended_dynamic_state::Device::load(instance, &device))
    } else {
        None
    };

    let data = DeviceData {
        device,
        get_proc_addr: get_device_proc_addr,
        supported_features,
        enabled_extensions: enabled_extensions_strs,
        extended_dynamic_state,
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


pub unsafe extern "system" fn allocate_command_buffer(
    device_handle: vk::Device,
    p_command_buffer_allocate_info: *const vk::CommandBufferAllocateInfo,
    p_command_buffers: *mut vk::CommandBuffer
) -> vk::Result {
    if device_handle == vk::Device::null() || p_command_buffer_allocate_info.is_null() {
        return vk::Result::ERROR_UNKNOWN;
    }
    let Ok(devices_guard) = devices().lock() else {
        log::error!("Could not lock devices");
        return vk::Result::ERROR_UNKNOWN;
    };
    let Some(device_data) = devices_guard.get(&device_handle) else {
        log::error!("Could not find device in map");
        return vk::Result::ERROR_UNKNOWN;
    };
    let res = unsafe { (device_data.device.fp_v1_0().allocate_command_buffers)(device_handle, p_command_buffer_allocate_info, p_command_buffers) };
    if res != vk::Result::SUCCESS {
        return res;
    }
    let Ok(mut cmd_buffers_guards) = cmd_buffers().lock() else {
        log::error!("Could not lock devices");
        return vk::Result::ERROR_UNKNOWN;
    };
    let cmd_buffers = unsafe { std::slice::from_raw_parts_mut(p_command_buffers, (*p_command_buffer_allocate_info).command_buffer_count as usize) };
    for cmd_buffer in cmd_buffers {
        cmd_buffers_guards.insert(*cmd_buffer, CommandBufferData {
            device_handle,
            cmd_pool: unsafe { (*p_command_buffer_allocate_info).command_pool }
        });
    }

    res
}

pub unsafe extern "system" fn free_command_buffers(
    device_handle: vk::Device,
    command_pool: vk::CommandPool,
    command_buffer_count: u32,
    p_command_buffers: *const vk::CommandBuffer,
) {
    if device_handle == vk::Device::null() {
        return;
    }
    let Ok(devices_guard) = devices().lock() else {
        log::error!("Could not lock devices");
        return;
    };
    let Some(device_data) = devices_guard.get(&device_handle) else {
        log::error!("Could not find device in map");
        return;
    };

    let Ok(mut cmd_buffers_guards) = cmd_buffers().lock() else {
        log::error!("Could not lock devices");
        return;
    };
    cmd_buffers_guards.retain(|_, cmd_buffer| cmd_buffer.cmd_pool == command_pool);
    unsafe { (device_data.device.fp_v1_0().free_command_buffers)(device_handle, command_pool, command_buffer_count, p_command_buffers) };
}

pub unsafe extern "system" fn reset_command_pool(
    device_handle: vk::Device,
    command_pool: vk::CommandPool,
    flags: vk::CommandPoolResetFlags,
) -> vk::Result {
    if device_handle == vk::Device::null() {
        return vk::Result::ERROR_UNKNOWN;
    }
    let Ok(devices_guard) = devices().lock() else {
        log::error!("Could not lock devices");
        return vk::Result::ERROR_UNKNOWN;
    };
    let Some(device_data) = devices_guard.get(&device_handle) else {
        log::error!("Could not find device in map");
        return vk::Result::ERROR_UNKNOWN;
    };
    let Ok(mut cmd_buffers_guards) = cmd_buffers().lock() else {
        log::error!("Could not lock devices");
        return vk::Result::ERROR_UNKNOWN;
    };
    cmd_buffers_guards.retain(|_, cmd_buffer| cmd_buffer.cmd_pool == command_pool);
    unsafe { (device_data.device.fp_v1_0().reset_command_pool)(device_handle, command_pool, flags) }
}

pub unsafe extern "system" fn destroy_command_pool(
    device_handle: vk::Device,
    command_pool: vk::CommandPool,
    p_allocator: *const vk::AllocationCallbacks,
) {
    if device_handle == vk::Device::null() {
        return;
    }
    let Ok(devices_guard) = devices().lock() else {
        log::error!("Could not lock devices");
        return;
    };
    let Some(device_data) = devices_guard.get(&device_handle) else {
        log::error!("Could not find device in map");
        return;
    };

    let Ok(mut cmd_buffers_guards) = cmd_buffers().lock() else {
        log::error!("Could not lock devices");
        return;
    };
    cmd_buffers_guards.retain(|_, cmd_buffer| cmd_buffer.cmd_pool == command_pool);
    unsafe { (device_data.device.fp_v1_0().destroy_command_pool)(device_handle, command_pool, p_allocator); }
}

pub unsafe extern "system" fn cmd_set_depth_bounds_test_enable<const EXT: bool>(
    command_buffer_handle: vk::CommandBuffer,
    enabled: vk::Bool32,
) {
    if command_buffer_handle == vk::CommandBuffer::null() {
        return;
    }
    let Ok(cmd_buffers_guards) = cmd_buffers().lock() else {
        log::error!("Could not lock devices");
        return;
    };
    let Some(cmd_buffer_data) = cmd_buffers_guards.get(&command_buffer_handle) else {
        log::error!("Could not find command buffer in map");
        return;
    };
    let Ok(devices_guard) = devices().lock() else {
        log::error!("Could not lock devices");
        return;
    };
    let Some(device_data) = devices_guard.get(&cmd_buffer_data.device_handle) else {
        log::error!("Could not find device in map");
        return;
    };
    if device_data.supported_features.depth_bounds == vk::FALSE {
        return;
    }


    if EXT {
        let Some(ext_device) = device_data.extended_dynamic_state.as_ref() else {
            log::error!("VK_EXT_extended_dynamic_state was not loaded.");
            return;
        };
        unsafe { (ext_device.fp().cmd_set_depth_bounds_test_enable_ext)(command_buffer_handle, enabled) }
    } else {
        unsafe { (device_data.device.fp_v1_3().cmd_set_depth_bounds_test_enable)(command_buffer_handle, enabled) }
    }
}

pub unsafe extern "system" fn create_graphics_pipelines(
    device_handle: vk::Device,
    pipeline_cache: vk::PipelineCache,
    create_info_count: u32,
    create_infos: *const vk::GraphicsPipelineCreateInfo,
    allocator: *const vk::AllocationCallbacks,
    pipelines: *mut vk::Pipeline,
) -> vk::Result {
    if device_handle.is_null() {
        return vk::Result::ERROR_UNKNOWN;
    }
    let Ok(devices_guard) = devices().lock() else {
        log::error!("Could not lock devices");
        return vk::Result::ERROR_UNKNOWN;
    };
    let Some(device_data) = devices_guard.get(&device_handle) else {
        log::error!("Could not find device in map");
        return vk::Result::ERROR_UNKNOWN;
    };

    if device_data.supported_features.depth_bounds == vk::TRUE || create_infos.is_null() || create_info_count == 0 {
        unsafe { return (device_data.device.fp_v1_0().create_graphics_pipelines)(device_handle, pipeline_cache, create_info_count, create_infos, allocator, pipelines) };
    }

    let create_infos = unsafe { std::slice::from_raw_parts(create_infos, create_info_count as usize) };

    let depth_stencil_infos: Vec<Option<vk::PipelineDepthStencilStateCreateInfo>> =
        create_infos.iter().map(|info| unsafe {
            info.p_depth_stencil_state.as_ref().map(|ds| vk::PipelineDepthStencilStateCreateInfo {
                depth_bounds_test_enable: vk::FALSE,
                ..ds.clone()
            })
        }).collect();

    let mut new_infos = Vec::<vk::GraphicsPipelineCreateInfo>::with_capacity(create_infos.len());
    for (info, ds) in create_infos.iter().zip(depth_stencil_infos.iter()) {
        let mut new_info = info.clone();
        if let Some(ds) = ds {
            new_info.p_depth_stencil_state = ds;
        }
        new_infos.push(new_info);
    }

    unsafe { (device_data.device.fp_v1_0().create_graphics_pipelines)(device_handle, pipeline_cache, create_info_count, new_infos.as_ptr(), allocator, pipelines) }
}
