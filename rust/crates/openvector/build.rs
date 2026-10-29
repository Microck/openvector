//! Compiles the pinned zlib 1.2.3 PNG compressor and IJG jpeg-6b JPEG
//! compressor. Upstream sources under `vendor/` are unmodified copies of the
//! files the JavaScript reference builds into WebAssembly.
fn main() {
    let zlib = "vendor/zlib-1.2.3";
    let mut deflate = cc::Build::new();
    deflate.include(zlib).define("NO_GZIP", None).define("NO_ERRNO_H", None).define("Z_PREFIX", None)
        .flag_if_supported("-std=gnu99").flag_if_supported("-Wno-deprecated-non-prototype").warnings(false)
        .file("csrc/bitmap_deflate.c");
    for name in ["adler32.c", "compress.c", "deflate.c", "trees.c", "zutil.c"] {
        deflate.file(format!("{zlib}/{name}"));
    }
    deflate.compile("ov_bitmap_deflate");

    let jpeg = "vendor/jpeg-6b";
    let mut compress = cc::Build::new();
    compress.include(jpeg).define("NO_GETENV", None)
        .flag_if_supported("-std=gnu99").flag_if_supported("-Wno-deprecated-non-prototype").warnings(false)
        .file("csrc/bitmap_jpeg.c");
    for name in ["jcapimin.c", "jcapistd.c", "jccoefct.c", "jccolor.c", "jcdctmgr.c", "jchuff.c", "jcinit.c",
        "jcmainct.c", "jcmarker.c", "jcmaster.c", "jcomapi.c", "jcparam.c", "jcphuff.c", "jcprepct.c", "jcsample.c",
        "jfdctflt.c", "jfdctfst.c", "jfdctint.c", "jmemmgr.c", "jmemnobs.c", "jutils.c"] {
        compress.file(format!("{jpeg}/{name}"));
    }
    compress.compile("ov_bitmap_jpeg");
    println!("cargo:rerun-if-changed=csrc");
    println!("cargo:rerun-if-changed=vendor");
}
