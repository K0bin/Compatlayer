#![allow(non_camel_case_types, non_snake_case, dead_code)]

// Layer declarations missing from ash

use ash::vk;
use std::ffi::{c_char, c_void};
use ash::vk::TaggedStructure;

pub const CURRENT_LOADER_LAYER_INTERFACE_VERSION: u32 = 2;

pub type PFN_GetPhysicalDeviceProcAddr =
    Option<unsafe extern "system" fn(vk::Instance, *const c_char) -> vk::PFN_vkVoidFunction>;

#[repr(u32)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VkNegotiateLayerStructType {
    LAYER_NEGOTIATE_UNINTIALIZED = 0,
    LAYER_NEGOTIATE_INTERFACE_STRUCT = 1,
}

#[repr(C)]
pub struct VkNegotiateLayerInterface {
    pub s_type: VkNegotiateLayerStructType,
    pub p_next: *mut c_void,
    pub loader_layer_interface_version: u32,
    pub pfn_get_instance_proc_addr: Option<vk::PFN_vkGetInstanceProcAddr>,
    pub pfn_get_device_proc_addr: Option<vk::PFN_vkGetDeviceProcAddr>,
    pub pfn_get_physical_device_proc_addr: PFN_GetPhysicalDeviceProcAddr,
}

#[repr(u32)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VkLayerFunction {
    LAYER_LINK_INFO = 0,
    LOADER_DATA_CALLBACK = 1,
    LOADER_LAYER_CREATE_DEVICE_CALLBACK = 2,
    LOADER_FEATURES = 3,
}

#[repr(C)]
pub struct VkLayerInstanceLink {
    pub p_next: *mut VkLayerInstanceLink,
    pub pfn_next_get_instance_proc_addr: vk::PFN_vkGetInstanceProcAddr,
    pub pfn_next_get_device_proc_addr: PFN_GetPhysicalDeviceProcAddr,
}

#[repr(C)]
pub struct VkLayerInstanceCreateInfo {
    pub s_type: vk::StructureType, // LOADER_INSTANCE_CREATE_INFO
    pub p_next: *const c_void,
    pub function: VkLayerFunction,
    pub contents: VkLayerInstanceCreateInfoContents
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct VkLayerDeviceFunctions {
    pub pfn_layer_create_device: vk::PFN_vkCreateDevice,
    pub pfn_layer_destroy_device: vk::PFN_vkDestroyDevice,
}

pub union VkLayerInstanceCreateInfoContents {
    pub p_layer_info: *mut VkLayerInstanceLink,
    pub pfn_set_instance_loader_data: vk::PFN_vkGetInstanceProcAddr,
    pub device_functions: VkLayerDeviceFunctions,
    pub features: u32
}

const _: () = {
    use std::mem::{offset_of, size_of};
    assert!(size_of::<VkLayerFunction>() == 4);
    assert!(offset_of!(VkLayerInstanceCreateInfo, function) == 16);
    assert!(offset_of!(VkLayerInstanceCreateInfo, contents) == 24);
    assert!(offset_of!(VkLayerInstanceCreateInfo, contents.p_layer_info) == 24);
    assert!(size_of::<VkLayerInstanceCreateInfo>() == 40);
};

unsafe impl TaggedStructure<'static> for VkLayerInstanceCreateInfo {
    const STRUCTURE_TYPE: vk::StructureType = vk::StructureType::LOADER_INSTANCE_CREATE_INFO;
}

#[repr(C)]
pub struct VkLayerDeviceLink {
    pub p_next: *mut VkLayerDeviceLink,
    pub pfn_next_get_instance_proc_addr: vk::PFN_vkGetInstanceProcAddr,
    pub pfn_next_get_device_proc_addr: vk::PFN_vkGetDeviceProcAddr,
}

#[repr(C)]
pub struct VkLayerDeviceCreateInfo {
    pub s_type: vk::StructureType, // LOADER_DEVICE_CREATE_INFO
    pub p_next: *const c_void,
    pub function: VkLayerFunction,
    pub p_layer_info: *mut VkLayerDeviceLink,
}

unsafe impl TaggedStructure<'static> for VkLayerDeviceCreateInfo {
    const STRUCTURE_TYPE: vk::StructureType = vk::StructureType::LOADER_DEVICE_CREATE_INFO;
}
