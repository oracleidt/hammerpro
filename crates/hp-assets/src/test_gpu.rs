use std::sync::OnceLock;

// Keep one Vulkan instance alive across parallel GPU reference tests. Creating
// and destroying independent instances during another test's rendering was
// associated with a native Vulkan loader access violation on the Windows host.
pub(crate) fn gpu() -> Option<(wgpu::Device, wgpu::Queue)> {
    static GPU: OnceLock<Option<(wgpu::Instance, wgpu::Device, wgpu::Queue)>> = OnceLock::new();
    GPU.get_or_init(|| {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).ok()?;
        let (device, queue) = pollster::block_on(
            adapter.request_device(&wgpu::DeviceDescriptor { required_limits: adapter.limits(), ..Default::default() }),
        )
        .expect("create the native-reference GPU device");
        Some((instance, device, queue))
    })
    .as_ref()
    .map(|(_, device, queue)| (device.clone(), queue.clone()))
}
