fn main() {
    #[cfg(target_os = "windows")]
    {
        embed_resource::compile("packaging/windows/MonitorSwitch.rc", embed_resource::NONE)
            .manifest_required()
            .expect("failed to embed the Windows application icon");
    }
}
