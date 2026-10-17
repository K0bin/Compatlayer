#![allow(non_camel_case_types, non_snake_case, dead_code)]

// Layer declarations missing from ash

use ash::vk;
use std::ffi::{c_char, c_void};

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
    pub sType: VkNegotiateLayerStructType,
    pub pNext: *mut c_void,
    pub loaderLayerInterfaceVersion: u32,
    pub pfnGetInstanceProcAddr: Option<vk::PFN_vkGetInstanceProcAddr>,
    pub pfnGetDeviceProcAddr: Option<vk::PFN_vkGetDeviceProcAddr>,
    pub pfnGetPhysicalDeviceProcAddr: PFN_GetPhysicalDeviceProcAddr,
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
    pub pNext: *mut VkLayerInstanceLink,
    pub pfnNextGetInstanceProcAddr: vk::PFN_vkGetInstanceProcAddr,
    pub pfnNextGetPhysicalDeviceProcAddr: PFN_GetPhysicalDeviceProcAddr,
}

#[repr(C)]
pub struct VkLayerInstanceCreateInfo {
    pub sType: vk::StructureType, // LOADER_INSTANCE_CREATE_INFO
    pub pNext: *const c_void,
    pub function: VkLayerFunction,
    pub pLayerInfo: *mut VkLayerInstanceLink,
}

#[repr(C)]
pub struct VkLayerDeviceLink {
    pub pNext: *mut VkLayerDeviceLink,
    pub pfnNextGetInstanceProcAddr: vk::PFN_vkGetInstanceProcAddr,
    pub pfnNextGetDeviceProcAddr: vk::PFN_vkGetDeviceProcAddr,
}

#[repr(C)]
pub struct VkLayerDeviceCreateInfo {
    pub sType: vk::StructureType, // LOADER_DEVICE_CREATE_INFO
    pub pNext: *const c_void,
    pub function: VkLayerFunction,
    pub pLayerInfo: *mut VkLayerDeviceLink,
}
