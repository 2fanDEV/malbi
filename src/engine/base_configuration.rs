use core::ffi;
use std::{
    borrow::Cow,
    io::{Cursor, Error, ErrorKind}, process::Command,
};

use ash::{
    ext::debug_utils,
    khr::{surface, swapchain},
    util::read_spv,
    vk::{
        self, ApplicationInfo, AttachmentDescription, AttachmentLoadOp, AttachmentReference, AttachmentStoreOp, BlendFactor, BlendOp, ColorComponentFlags, ColorSpaceKHR, CommandBufferAllocateInfo, CommandBufferBeginInfo, CommandBufferLevel, CommandBufferUsageFlags, CommandPool, CommandPoolCreateFlags, CommandPoolCreateInfo, ComponentMapping, ComponentSwizzle, CompositeAlphaFlagsKHR, CullModeFlags, DebugUtilsMessageSeverityFlagsEXT, DebugUtilsMessageTypeFlagsEXT, DebugUtilsMessengerCreateInfoEXT, DebugUtilsMessengerEXT, DeviceCreateInfo, DeviceQueueCreateInfo, DynamicState, Extent2D, Format, Framebuffer, FramebufferCreateInfo, FrontFace, GraphicsPipelineCreateInfo, Handle, ImageAspectFlags, ImageLayout, ImageSubresourceRange, ImageUsageFlags, ImageView, ImageViewCreateInfo, InstanceCreateFlags, InstanceCreateInfo, LogicOp, Offset2D, PhysicalDevice, PhysicalDeviceType, Pipeline, PipelineBindPoint, PipelineCache, PipelineColorBlendAttachmentState, PipelineColorBlendStateCreateInfo, PipelineDynamicStateCreateInfo, PipelineInputAssemblyStateCreateInfo, PipelineLayout, PipelineMultisampleStateCreateInfo, PipelineRasterizationStateCreateInfo, PipelineShaderStageCreateFlags, PipelineShaderStageCreateInfo, PipelineVertexInputStateCreateInfo, PipelineViewportStateCreateInfo, PolygonMode, PresentModeKHR, PrimitiveTopology, Queue, QueueFlags, Rect2D, RenderPass, RenderPassCreateInfo, SampleCountFlags, ShaderModuleCreateFlags, ShaderModuleCreateInfo, ShaderStageFlags, SharingMode, SubpassDescription, SurfaceCapabilitiesKHR, SurfaceFormatKHR, SurfaceKHR, SwapchainCreateInfoKHR, SwapchainKHR, Viewport, KHR_SWAPCHAIN_NAME
    },
    Device, Entry, Instance,
};
use winit::{
    raw_window_handle::{HasDisplayHandle, HasWindowHandle},
    window::Window,
};

pub struct BaseConfig {
    instance: Instance,
    debug_instance: debug_utils::Instance,
    debug_utils_messenger: DebugUtilsMessengerEXT,

    physical_device: PhysicalDevice,
    device: Device,

    queue_family_indexes: Vec<usize>,
    graphics_queue: Queue,
    presentation_queue: Option<Queue>,

    surface: SurfaceKHR,
    surface_instance: surface::Instance,
    surface_capabilities: SurfaceCapabilitiesKHR,
    surface_format: SurfaceFormatKHR,
    image_count: u32,

    swapchain_device: swapchain::Device,
    swapchain: SwapchainKHR,
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

            let surface = ash_window::create_surface(
                &entry,
                &instance,
                window.display_handle().unwrap().as_raw(),
                window.window_handle().unwrap().as_raw(),
                None,
            )
            .expect("Failed to create surface");

            let debug_instance = debug_utils::Instance::new(&entry, &instance);
            let debug_utils_messenger = debug_instance
                .create_debug_utils_messenger(&debug_info, None)
                .expect("Failed to create debug messenger");
            let surface_instance = surface::Instance::new(&entry, &instance);

            let physical_device = Self::create_physical_device(&instance, QueueFlags::GRAPHICS)
                .expect("Failed to create a physical device");

            let (device, queue_family_indexes) =
                Self::create_device(&instance, physical_device, QueueFlags::GRAPHICS)
                    .expect("Failed to create physical or logical device or the queue");

            let (
                (graphics_queue, _graphics_queue_idx),         // TODO
                (presentation_queue, _presentation_queue_idx), // TODO
            ) = Self::get_device_queues(
                device.clone(),
                queue_family_indexes.clone(),
                &surface_instance,
                physical_device,
                surface,
            );

            let swapchain_device = swapchain::Device::new(&instance, &device);

            let surface_capabilities = surface_instance
                .get_physical_device_surface_capabilities(physical_device, surface)
                .expect("Failed to load physical device surface capabilities");

            let (surface_formats, present_modes) = Self::query_swapchain_support_details(
                surface_instance.clone(),
                &physical_device,
                surface,
            );

            let surface_format = *surface_formats
                .clone()
                .iter()
                .find(|&format| {
                    format.format.eq(&Format::B8G8R8A8_SRGB)
                        && format.color_space.eq(&ColorSpaceKHR::SRGB_NONLINEAR)
                })
                .unwrap_or(surface_formats.get_unchecked(0));

            let present_mode = *present_modes
                .iter()
                .find(|&present_mode| present_mode.eq(&PresentModeKHR::MAILBOX))
                .unwrap_or(&PresentModeKHR::FIFO);

            let mut swap_extent = surface_capabilities.current_extent;
            let window_dimensions = window.inner_size();
            swap_extent = swap_extent
                .width(window_dimensions.width.clamp(
                    surface_capabilities.min_image_extent.width,
                    surface_capabilities.max_image_extent.width,
                ))
                .height(window_dimensions.height.clamp(
                    surface_capabilities.min_image_extent.height,
                    surface_capabilities.max_image_extent.height,
                ));

            let min_image_count = surface_capabilities.min_image_count + 1;
            let max_image_count = surface_capabilities.max_image_count;

            let desired_image_count = if max_image_count > 0 && min_image_count > max_image_count {
                max_image_count
            } else {
                min_image_count
            };

            let swapchain_create_info = SwapchainCreateInfoKHR::default()
                .image_color_space(surface_format.color_space)
                .image_format(surface_format.format)
                .min_image_count(desired_image_count)
                .image_extent(swap_extent)
                .image_usage(ImageUsageFlags::COLOR_ATTACHMENT)
                .surface(surface)
                .image_sharing_mode(SharingMode::EXCLUSIVE)
                .pre_transform(surface_capabilities.current_transform)
                .composite_alpha(CompositeAlphaFlagsKHR::OPAQUE)
                .image_array_layers(1)
                .present_mode(present_mode);

            let swapchain = swapchain_device
                .create_swapchain(&swapchain_create_info, None)
                .expect("Failed to create swapchain");

            let swapchain_images = swapchain_device
                .get_swapchain_images(swapchain)
                .expect("Failed to get swapchain images");

            let swapchain_image_views = swapchain_images
                .iter()
                .map(|image| {
                    let create_info = ImageViewCreateInfo::default()
                        .image(*image)
                        .components(
                            ComponentMapping::default()
                                .r(ComponentSwizzle::R)
                                .b(ComponentSwizzle::B)
                                .g(ComponentSwizzle::G)
                                .a(ComponentSwizzle::A),
                        )
                        .subresource_range(
                            ImageSubresourceRange::default()
                                .aspect_mask(ImageAspectFlags::COLOR)
                                .base_mip_level(0)
                                .level_count(1)
                                .base_array_layer(0)
                                .layer_count(1),
                        );
                    device
                        .create_image_view(&create_info, None)
                        .expect("Failed to create image views")
                })
                .collect::<Vec<ImageView>>();

            let render_pass = create_render_pass(&device, surface_format.format)
                .expect("Failed to create render pass");

            let graphics_pipeline = create_graphics_pipeline(&device, swap_extent, render_pass)
                .expect("Failed to create graphic pipeline");

            let framebuffers = create_framebuffers(&device, render_pass, swapchain_image_views, swap_extent);

            let command_pool = create_command_pool(&instance);
            


            Ok(Self {
                instance: instance,
                debug_instance: debug_instance,
                debug_utils_messenger: debug_utils_messenger,
                physical_device: physical_device,
                device: device,
                queue_family_indexes: queue_family_indexes,
                graphics_queue: graphics_queue,
                presentation_queue: presentation_queue,
                surface: surface,
                surface_instance: surface_instance,
                surface_capabilities: surface_capabilities,
                surface_format: surface_format,
                image_count: desired_image_count,
                swapchain: swapchain,
                swapchain_device: swapchain_device,
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
            for _name in used_layer_names {
                match layer_properties.iter().find(|&layer_property| {
                    !layer_property
                        .layer_name_as_c_str()
                        .expect("failed to query layer property")
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

    fn create_device(
        instance: &Instance,
        physical_device: PhysicalDevice,
        queue_flag: QueueFlags,
    ) -> Result<(Device, Vec<usize>), Error> {
        unsafe {
            let queue_family_indexes =
                Self::find_queue_family_index(instance, &physical_device, queue_flag)
                    .expect("Failed to find queue families");
            println!("SIZE: {:?}", queue_family_indexes);
            let device_queue_infos = queue_family_indexes
                .iter()
                .map(|idx| {
                    DeviceQueueCreateInfo::default()
                        .queue_family_index(*idx as u32)
                        .queue_priorities(&[1.0])
                })
                .collect::<Vec<DeviceQueueCreateInfo>>();

            let physical_devices_feature = &instance.get_physical_device_features(physical_device);

            let mut enabled_extension_names = Vec::new();
            enabled_extension_names.push(ash::vk::KHR_PORTABILITY_SUBSET_NAME.as_ptr());
            enabled_extension_names.push(KHR_SWAPCHAIN_NAME.as_ptr());

            let device_create_info = DeviceCreateInfo::default()
                .enabled_extension_names(&enabled_extension_names)
                .enabled_features(&physical_devices_feature)
                .queue_create_infos(&device_queue_infos);

            let device = instance
                .create_device(physical_device, &device_create_info, None)
                .expect("Failed to create a logical device");

            Ok((device, queue_family_indexes))
        }
    }

    fn create_physical_device(
        instance: &Instance,
        queue_flag: QueueFlags,
    ) -> Result<PhysicalDevice, Error> {
        unsafe {
            let enumerated_physical_devices = instance
                .enumerate_physical_devices()
                .expect("Failed to enumerate physical devices");
            let mut phy_device: Option<PhysicalDevice> = None;

            for physical_device in enumerated_physical_devices {
                if Self::physical_device_suitability(instance, physical_device, queue_flag) {
                    phy_device = Some(physical_device);
                    break;
                }
            }

            return match phy_device {
                Some(physical_device) => Ok(physical_device),
                None => Err(Error::new(
                    ErrorKind::NotFound,
                    "No suitable physical device found!",
                )),
            };
        }
    }

    fn physical_device_suitability(
        instance: &Instance,
        physical_device: vk::PhysicalDevice,
        queue_flag: QueueFlags,
    ) -> bool {
        unsafe {
            let physical_device_properties =
                instance.get_physical_device_properties(physical_device);

            return if physical_device_properties.device_type == PhysicalDeviceType::INTEGRATED_GPU
                && Self::find_queue_family_index(instance, &physical_device, queue_flag).is_ok()
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
    ) -> Result<Vec<usize>, Error> {
        unsafe {
            let idxs = instance
                .get_physical_device_queue_family_properties(*physical_device)
                .iter()
                .enumerate()
                .filter(|(_idx, queue_property)| queue_property.queue_flags.contains(queue_flag))
                .map(|entry| entry.0)
                .collect::<Vec<usize>>();
            Ok(idxs)
        }
    }

    fn create_queue(device: &Device, queue_family_idx: u32) -> Queue {
        unsafe { device.get_device_queue(queue_family_idx, 0) }
    }

    fn get_device_queues(
        device: Device,
        queue_family_indexes: Vec<usize>,
        surface_instance: &surface::Instance,
        physical_device: PhysicalDevice,
        surface: vk::SurfaceKHR,
    ) -> ((Queue, usize), (Option<Queue>, usize)) {
        unsafe {
            let graphics_queue = Self::create_queue(&device, queue_family_indexes[0] as u32);
            let mut presentation_queue: Option<Queue> = None;
            if queue_family_indexes.len() > 1
                && !queue_family_indexes[0].eq(&queue_family_indexes[1])
            {
                let present_support = surface_instance
                    .get_physical_device_surface_support(
                        physical_device,
                        queue_family_indexes[1] as u32,
                        surface,
                    )
                    .expect("failed to find surface support for queue index");

                if present_support {
                    presentation_queue =
                        Some(Self::create_queue(&device, queue_family_indexes[1] as u32));
                }
            }
            (
                (graphics_queue, queue_family_indexes[0]),
                (presentation_queue, queue_family_indexes[1]),
            )
        }
    }

    fn query_swapchain_support_details(
        surface_instance: surface::Instance,
        physical_device: &PhysicalDevice,
        surface: vk::SurfaceKHR,
    ) -> (Vec<SurfaceFormatKHR>, Vec<PresentModeKHR>) {
        unsafe {
            let formats = surface_instance
                .get_physical_device_surface_formats(*physical_device, surface)
                .expect("Failed to retrieve device surface formats");

            let present_mode = surface_instance
                .get_physical_device_surface_present_modes(*physical_device, surface)
                .expect("Failed to retrieve present modes");

            (formats, present_mode)
        }
    }
}

fn create_command_pool(device: &Device, queue_family_index : i32) -> CommandPool {
    let command_pool_create_info = CommandPoolCreateInfo::default().flags(CommandPoolCreateFlags::RESET_COMMAND_BUFFER).queue_family_index(queue_family_index);
    unsafe { device.create_command_pool(&command_pool_create_info, None).expect("Failed to initialize command pool") } 
}

fn create_command_buffer(device: &Device, command_pool: CommandPool) -> CommandBuffer { 
    let command_buffer_create_info = CommandBufferAllocateInfo::default().command_pool(command_pool).level(CommandBufferLevel::PRIMARY);
    unsafe { device.allocate_command_buffers(&command_buffer_create_info).expect("Failed to create command buffer") }
}

fn create_framebuffers(
    device: &Device,
    render_pass: RenderPass,
    swapchain_images: Vec<ImageView>,
    swapchain_extent: Extent2D,
) -> Vec<Framebuffer> {
    let mut framebuffers = Vec::new();
    unsafe { 
    for image_view in swapchain_images {
        let image_view_vec = vec![image_view];
        let frame_buffer_create_info = FramebufferCreateInfo::default()
            .attachments(&image_view_vec)
            .render_pass(render_pass)
            .width(swapchain_extent.width)
            .height(swapchain_extent.height)
            .layers(1);

        let framebuffer = device
            .create_framebuffer(&frame_buffer_create_info, None)
            .expect("Failed to create frame_buffer");

        framebuffers.push(framebuffer);
    }
    framebuffers
}

fn create_render_pass(
    device: &Device,
    swapchain_image_format: Format,
) -> Result<RenderPass, vk::Result> {
    unsafe {
        let attachment_descriptions = vec![AttachmentDescription::default()
            .format(swapchain_image_format)
            .samples(SampleCountFlags::TYPE_1)
            .load_op(AttachmentLoadOp::CLEAR)
            .store_op(AttachmentStoreOp::STORE)
            .stencil_load_op(AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(AttachmentStoreOp::DONT_CARE)
            .initial_layout(ImageLayout::UNDEFINED)
            .final_layout(ImageLayout::PRESENT_SRC_KHR)];

        let attachment_reference = vec![AttachmentReference::default()
            .attachment(0)
            .layout(ImageLayout::COLOR_ATTACHMENT_OPTIMAL)];
        let subpass_description = vec![SubpassDescription::default()
            .pipeline_bind_point(PipelineBindPoint::GRAPHICS)
            .color_attachments(&attachment_reference)];

        let render_pass_create_info = RenderPassCreateInfo::default()
            .subpasses(&subpass_description)
            .attachments(&attachment_descriptions);
        device.create_render_pass(&render_pass_create_info, None)
    }
}

fn create_graphics_pipeline(
    device: &Device,
    swapchain_extend: Extent2D,
    render_pass: RenderPass,
) -> Result<Vec<Pipeline>, (Vec<Pipeline>, vk::Result)> {
    unsafe {
        let mut fragment_spv = Cursor::new(include_bytes!("../../shader/colors.spv").as_ref());
        let mut vert_spv = Cursor::new(include_bytes!("../../shader/triangle.spv").as_ref());

        let mut vert_code = read_spv(&mut vert_spv).expect("Failed to convert to code");
        let mut frag_code = read_spv(&mut fragment_spv).expect("Failed to convert to code");

        let vert_shader_create_info = ShaderModuleCreateInfo::default()
            .code(&vert_code)
            .flags(ShaderModuleCreateFlags::empty());
        let frag_shader_create_info = ShaderModuleCreateInfo::default()
            .code(&frag_code)
            .flags(ShaderModuleCreateFlags::empty());

        let vert_shader_module = device
            .create_shader_module(&vert_shader_create_info, None)
            .expect("Failed to create shader");
        let fragment_shader_module = device
            .create_shader_module(&frag_shader_create_info, None)
            .expect("Failed to create shader");

        let vert_shader_stage_info = PipelineShaderStageCreateInfo::default()
            .stage(ShaderStageFlags::VERTEX)
            .module(vert_shader_module);
        let frag_shader_stage_info = PipelineShaderStageCreateInfo::default()
            .stage(ShaderStageFlags::FRAGMENT)
            .module(fragment_shader_module);

        let shader_stages = vec![vert_shader_stage_info, frag_shader_stage_info];

        let dynamic_states = vec![DynamicState::VIEWPORT, DynamicState::SCISSOR];

        let pipeline_vertex_info = PipelineVertexInputStateCreateInfo::default();

        let pipeline_input_assembly_info = PipelineInputAssemblyStateCreateInfo::default()
            .topology(PrimitiveTopology::TRIANGLE_LIST)
            .primitive_restart_enable(false);

        let viewport = Viewport::default()
            .x(0.0)
            .y(0.0)
            .width(swapchain_extend.width as f32)
            .height(swapchain_extend.height as f32)
            .min_depth(0.0)
            .max_depth(1.0);

        let viewports = vec![viewport];

        let scissor = vec![Rect2D::default()
            .offset(Offset2D::default().x(0).y(0))
            .extent(swapchain_extend)];

        let dynamic_states_info =
            PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

        let viewports_pipeline_create_info = PipelineViewportStateCreateInfo::default()
            .viewports(&viewports)
            .scissors(&scissor);

        let pipeline_rasterization_create_info = PipelineRasterizationStateCreateInfo::default()
            .depth_clamp_enable(false)
            .rasterizer_discard_enable(false)
            .polygon_mode(PolygonMode::FILL)
            .line_width(1.0)
            .cull_mode(CullModeFlags::BACK)
            .front_face(FrontFace::CLOCKWISE)
            .depth_bias_clamp(0.0)
            .depth_bias_constant_factor(0.0)
            .depth_bias_slope_factor(0.0);

        let multisample_create_info = PipelineMultisampleStateCreateInfo::default()
            .sample_shading_enable(false)
            .rasterization_samples(SampleCountFlags::TYPE_1)
            .min_sample_shading(1.0)
            .alpha_to_coverage_enable(false)
            .alpha_to_one_enable(false);

        let pipeline_color_blend_attachment = PipelineColorBlendAttachmentState::default()
            .blend_enable(false)
            .src_color_blend_factor(BlendFactor::ONE)
            .dst_color_blend_factor(BlendFactor::ZERO)
            .color_blend_op(BlendOp::ADD)
            .src_alpha_blend_factor(BlendFactor::ONE)
            .dst_alpha_blend_factor(BlendFactor::ZERO)
            .alpha_blend_op(BlendOp::ADD)
            .color_write_mask(
                ColorComponentFlags::R
                    | ColorComponentFlags::G
                    | ColorComponentFlags::B
                    | ColorComponentFlags::A,
            );

        let color_blending = PipelineColorBlendStateCreateInfo::default()
            .logic_op_enable(false)
            .logic_op(LogicOp::COPY)
            .blend_constants([0.0, 0.0, 0.0, 0.0]);

        let pipeline_layout = PipelineLayout::default();

        let graphics_pipeline_create_info = vec![GraphicsPipelineCreateInfo::default()
            .vertex_input_state(&pipeline_vertex_info)
            .input_assembly_state(&pipeline_input_assembly_info)
            .viewport_state(&viewports_pipeline_create_info)
            .rasterization_state(&pipeline_rasterization_create_info)
            .multisample_state(&multisample_create_info)
            .color_blend_state(&color_blending)
            .dynamic_state(&dynamic_states_info)
            .render_pass(render_pass)
            .subpass(0)
            .base_pipeline_handle(Pipeline::null())];

        device.create_graphics_pipelines(
            PipelineCache::null(),
            &graphics_pipeline_create_info,
            None,
        )
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
