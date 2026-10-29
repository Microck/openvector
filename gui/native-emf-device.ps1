$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class OpenVectorEmfDevice {
    [DllImport("gdi32.dll", CharSet=CharSet.Unicode)]
    private static extern IntPtr CreateEnhMetaFileW(IntPtr reference, string filename, IntPtr bounds, string description);
    [DllImport("gdi32.dll")]
    private static extern IntPtr CloseEnhMetaFile(IntPtr dc);
    [DllImport("gdi32.dll")]
    private static extern uint GetEnhMetaFileHeader(IntPtr metafile, uint size, [Out] byte[] header);
    [DllImport("gdi32.dll")]
    private static extern bool DeleteEnhMetaFile(IntPtr metafile);

    // A memory-only empty metafile asks GDI for its actual current reference
    // device, including micrometers. No window, file or picture is created.
    public static byte[] Read() {
        IntPtr dc = CreateEnhMetaFileW(IntPtr.Zero, null, IntPtr.Zero, null);
        if (dc == IntPtr.Zero) throw new InvalidOperationException("GDI could not create an EMF device context.");
        IntPtr metafile = CloseEnhMetaFile(dc);
        if (metafile == IntPtr.Zero) throw new InvalidOperationException("GDI could not close the EMF device context.");
        try {
            byte[] header = new byte[108];
            if (GetEnhMetaFileHeader(metafile, 108, header) != 108)
                throw new InvalidOperationException("GDI did not return the full EMF device header.");
            return header;
        } finally { DeleteEnhMetaFile(metafile); }
    }
}
'@
$header = [OpenVectorEmfDevice]::Read()
@{
    pixels = @([BitConverter]::ToInt32($header,72),[BitConverter]::ToInt32($header,76))
    millimeters = @([BitConverter]::ToInt32($header,80),[BitConverter]::ToInt32($header,84))
    micrometers = @([BitConverter]::ToInt32($header,100),[BitConverter]::ToInt32($header,104))
} | ConvertTo-Json -Compress
