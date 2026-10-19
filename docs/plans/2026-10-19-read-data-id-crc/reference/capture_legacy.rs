    #[test]
    fn capture_legacy_bad_id_controller() {
        use emu198x_shell::{MediaImage, MediaKind, MediaSet};
        let out = std::path::Path::new("/Users/stevehill/Projects/198x/output/qualification/read-data-id-crc-reference/legacy");
        std::fs::create_dir_all(out).expect("capture directory");
        for op in [0x46, 0x4c] {
            for accepted in [0, 179] {
                for at_request in [false, true] {
                    let (_,mut source)=writable_sector_runtime(false);
                    source[0x11c]=0x20;source[0x11d]=if op==0x4c {0x40}else{0};
                    let mut runtime=SpectrumPlus3Runtime::blank();
                    let mut media=MediaSet::new();media.push(MediaImage::new("disk-a",MediaKind::Disk,&source));
                    runtime.load_media(&media).expect("mount");
                    send_fdc_command(&mut runtime,&[op,0,0,0,1,2,1,0x2a,0xff]);
                    for _ in 0..accepted {assert_eq!(read_fdc_byte(&mut runtime),0xe5);}
                    if at_request {await_fdc_read_request(&mut runtime);}
                    let bytes=postcard::to_allocvec(&runtime.machine().fdc).expect("controller only");
                    std::fs::write(out.join(format!("{op:02x}-{accepted}-{}.postcard",u8::from(at_request))),bytes).expect("capture controller");
                }
            }
        }
    }

