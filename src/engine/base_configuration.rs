use core::ffi;
use std::{
    borrow::Cow,
    io::{Error, ErrorKind},
};

use ash::{
    ext::debug_utils,
    khr::{surface, swapchain},
    vk::{
        self, ApplicationInfo, DebugUtilsMessageSeverityFlagsEXT, DebugUtilsMessageTypeFlagsEXT,
        DebugUtilsMessengerCreateInfoEXT, DebugUtilsMessengerEXT, DeviceCreateInfo,
        DeviceQueueCreateInfo, InstanceCreateFlags, InstanceCreateInfo, PhysicalDevice,
        PhysicalDeviceType, QueueFlags,
    },
    Device, Entry, Instance,
};
use winit::{
    raw_window_handle::{HasDisplayHandle, HasWindowHandle},
    window::Window,
};

use super::app;

pub struct BaseConfig {
    instance: Instance,
    debug_instance: debug_utils::Instance,
    debug_utils_messenger: DebugUtilsMessengerEXT,
}

impl BaseConfig {
    pub fn init(window: &mut Window) -> Result<BaseConfig, Error> {
        unsafe {
            let entry = Entry::load().expect("No vulkan library found on this machine");

            let mut debug_info = DebugUtilsMessengerCreateInfoEXT::default()
                .message_severity(
                    DebugUtilsMessageSeverityFlagsEXT::ERROR
                        | DebugUtilsMessageSeverityFlagsEXT::WARNING
                        | DebugUtilsMessageSeverityFlagsEXT::INFO
                        | DebugUtilsMessageSeverityFlagsEXT::VERBOSE,
                )
                .message_type(
                    DebugUtilsMessageTypeFlagsEXT::GENERAL
                        | DebugUtilsMessageTypeFlagsEXT::VALIDATION
                        | DebugUtilsMessageTypeFlagsEXT::PERFORMANCE
                        | DebugUtilsMessageTypeFlagsEXT::DEVICE_ADDRESS_BINDING,
                )
                .pfn_user_callback(Some(debug_callback));

            let instance = Self::create_instance(window, &entry, &mut debug_info)
                .expect("Failed to create instance");

            let debug_instance = debug_utils::Instance::new(&entry, &instance);
            let debug_utils_messenger = debug_instance
                .create_debug_utils_messenger(&debug_info, None)
                .expect("Failed to create debug messenger");

            let device = create_device(&instance, QueueFlags::GRAPHICS)
                .expect("Failed to create logical device");

            let surface_instance = surface::Instance::new(&entry, &instance);
            let swapchain_instance = swapchain::Instance::new(&entry, &instance);

            let surface = ash_window::create_surface(
                &entry,
                &instance,
                window.display_handle().unwrap().as_raw(),
                window.window_handle().unwrap().as_raw(),
                None,
            )
            .expect("Failed to create surface");

            Ok(Self {
                instance: instance,
                debug_instance: debug_instance,
                debug_utils_messenger: debug_utils_messenger,
            })
        }
    }

    fn create_instance(
        window: &mut Window,
        entry: &Entry,
        debug_info: &mut DebugUtilsMessengerCreateInfoEXT,
    ) -> Result<Instance, Error> {
        unsafe {
            let application_name = b"Malbi\0";
            let app_info = ApplicationInfo::default()
                .api_version(0)
                .engine_name(ffi::CStr::from_bytes_with_nul_unchecked(b"No Engine\0"))
                .engine_version(1)
                .application_version(1)
                .application_name(ffi::CStr::from_bytes_with_nul_unchecked(application_name));

            let raw_display_handle = window
                .display_handle()
                .expect("failed to retrieve display handle")
                .as_raw();

            let enumerate_required_extensions =
                ash_window::enumerate_required_extensions(raw_display_handle)
                    .expect("Failed to enumerate required extensions");

            let mut required_extensions = enumerate_required_extensions.to_vec();
            required_extensions.push(ash::vk::KHR_PORTABILITY_ENUMERATION_NAME.as_ptr());
            required_extensions.push(debug_utils::NAME.as_ptr());

            let validation_layer = [ffi::CStr::from_bytes_with_nul_unchecked(
                b"VK_LAYER_KHRONOS_validation\0",
            )];

            let layer_names = validation_layer.map(|layer| layer.as_ptr()).to_vec();
            let validation_layers_enabled =
                Self::check_validation_layer_support(&entry, &layer_names);

            let mut instance_create_info = InstanceCreateInfo::default()
                .application_info(&app_info)
                .enabled_extension_names(&required_extensions)
                .flags(InstanceCreateFlags::ENUMERATE_PORTABILITY_KHR)
                .push_next(debug_info);

            if validation_layers_enabled {
                instance_create_info = instance_create_info.enabled_layer_names(&layer_names);
                println!("XDD {:?}", instance_create_info.enabled_layer_count);
            }

            let instance = entry
                .create_instance(&instance_create_info, None)
                .expect("Failed to create instance");

            Ok(instance)
        }
    }

    fn check_validation_layer_support(entry: &Entry, used_layer_names: &[*const i8]) -> bool {
        unsafe {
            let layer_properties = entry
                .enumerate_instance_layer_properties()
                .expect("Failed to enumerate instance layer properties");

            let mut flag = false;
            for name in used_layer_names {
                match layer_properties.iter().find(|&layer_property| {
                    !layer_property
                        .layer_name_as_c_str()
                        .expect("failed to create layer property")
                        .is_empty()
                }) {
                    Some(_layer_prop) => {
                        flag = true;
                        break;
                    }
                    None => {
                        flag = false;
                    }
                };
            }
            flag
        }
    }
}

fn create_device(
    instance: &Instance,
    queue_flag: QueueFlags,
) -> Result<(PhysicalDevice, Device), Error> {
    unsafe {
        let enumerated_physical_devices = instance
            .enumerate_physical_devices()
            .expect("Failed to enumerate physical devices");
        let mut phy_device: Option<PhysicalDevice> = None;
        for physical_device in enumerated_physical_devices {
            if physical_device_suitability(instance, physical_device, queue_flag) {
                phy_device = Some(physical_device);
                break;
            }
        }

        return match phy_device {
            Some(physical_device) => {
                let queue_create_info =
                    vec![
                        match find_queue_family_index(instance, &physical_device, queue_flag) {
                            Some(idx) => Ok(DeviceQueueCreateInfo::default()
                                .queue_family_index(idx as u32)
                                .queue_priorities(&[1.0])),
                            None => Err(Error::new(
                                ErrorKind::NotFound,
                                "No suitable physical device found!",
                            )),
                        }
                        .expect("Failed to create DeviceQueueCreateInfos"),
                    ];

                let physical_devices_feature =
                    &instance.get_physical_device_features(phy_device.unwrap());
                let device_create_info = DeviceCreateInfo::default()
                    .enabled_features(&physical_devices_feature)
                    .queue_create_infos(&queue_create_info);

                Ok((
                    physical_device,
                    instance
                        .create_device(physical_device, &device_create_info, None)
                        .expect("Failed to create a logical device"),
                ))
            }
            None => Err(Error::new(
                ErrorKind::NotFound,
                "No suitable physical device found!",
            )),
        };
    };
}

fn physical_device_suitability(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    queue_flag: QueueFlags,
) -> bool {
    unsafe {
        let physical_device_properties = instance.get_physical_device_properties(physical_device);

        return if physical_device_properties.device_type == PhysicalDeviceType::INTEGRATED_GPU
            && find_queue_family_index(instance, &physical_device, queue_flag).is_some()
        {
            true
        } else {
            false
        };
    }
}

fn find_queue_family_index(
    instance: &Instance,
    physical_device: &PhysicalDevice,
    queue_flag: QueueFlags,
) -> Option<usize> {
    unsafe {
        instance
            .get_physical_device_queue_family_properties(*physical_device)
            .iter()
            .enumerate()
            .find_map(|(idx, queue_property)| {
                if queue_property.queue_flags.contains(queue_flag) {
                    Some(idx)
                } else {
                    None
                }
            })
    }
}

unsafe extern "system" fn debug_callback(
    message_severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    message_type: vk::DebugUtilsMessageTypeFlagsEXT,
    p_callback_data: *const vk::DebugUtilsMessengerCallbackDataEXT<'_>,
    _user_data: *mut std::os::raw::c_void,
) -> u32 {
    let callback_data = *p_callback_data;
    let message_id_number = callback_data.message_id_number;
    let message_id_name = if callback_data.p_message_id_name.is_null() {
        Cow::from("")
    } else {
        ffi::CStr::from_ptr(callback_data.p_message_id_name).to_string_lossy()
    };

    let message = if callback_data.p_message.is_null() {
        Cow::from("")
    } else {
        ffi::CStr::from_ptr(callback_data.p_message).to_string_lossy()
    };
    println!(
        "{message_severity:?}:{message_type:?}:{message_id_name} {message_id_number}:{message}\n"
    );
    vk::FALSE
}

impl Drop for BaseConfig {
    fn drop(&mut self) {
        unsafe {
            self.instance.destroy_instance(None);
            self.debug_instance
                .destroy_debug_utils_messenger(self.debug_utils_messenger, None);
        };
    }
}
