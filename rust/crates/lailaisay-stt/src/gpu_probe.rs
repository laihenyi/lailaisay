pub fn detect_gpu() -> Result<(usize, String), Box<dyn std::error::Error>> {
    use ash::vk;
    // All Vulkan interaction is isolated from the UI process, including loader faults.
    unsafe {
        let entry = ash::Entry::load()?;
        let app = vk::ApplicationInfo::default().api_version(vk::API_VERSION_1_2);
        let instance = entry.create_instance(
            &vk::InstanceCreateInfo::default().application_info(&app),
            None,
        )?;
        let result = (|| {
            let devices = instance.enumerate_physical_devices()?;
            let mut candidates = Vec::new();
            for (index, device) in devices.into_iter().enumerate() {
                let props = instance.get_physical_device_properties(device);
                let name = std::ffi::CStr::from_ptr(props.device_name.as_ptr())
                    .to_string_lossy()
                    .into_owned();
                let memory = instance.get_physical_device_memory_properties(device);
                let bytes: u64 = memory.memory_heaps[..memory.memory_heap_count as usize]
                    .iter()
                    .filter(|h| h.flags.contains(vk::MemoryHeapFlags::DEVICE_LOCAL))
                    .map(|h| h.size)
                    .sum();
                eprintln!(
                    "[lailaisay-worker] detected {name}, type={:?}, local_memory={} MiB",
                    props.device_type,
                    bytes / 1024 / 1024
                );
                let rank = match props.device_type {
                    vk::PhysicalDeviceType::DISCRETE_GPU => 2,
                    vk::PhysicalDeviceType::INTEGRATED_GPU => 1,
                    _ => continue, // Do not mistake a software Vulkan driver for GPU acceleration.
                };
                if instance
                    .get_physical_device_queue_family_properties(device)
                    .iter()
                    .any(|q| q.queue_count > 0 && q.queue_flags.contains(vk::QueueFlags::COMPUTE))
                {
                    candidates.push((rank, bytes, index, name));
                }
            }
            candidates.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
            candidates
                .into_iter()
                .next()
                .map(|(_, _, index, name)| (index, name))
                .ok_or_else(|| "no hardware GPU with a Vulkan compute queue".into())
        })();
        instance.destroy_instance(None);
        result
    }
}
