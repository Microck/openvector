param([Parameter(Mandatory=$true)][string]$Filename)

Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;

public sealed class ExportAccessEntry {
    public uint desiredAccess;
    public bool opened;
    public int error;
    public bool sizeReadable;
    public int sizeError;
    public long size;
    public bool closed;
    public int closeError;
}

public static class ExportFileAccessQuery {
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
    private static extern IntPtr CreateFileW(string filename, uint access, uint share,
        IntPtr security, uint disposition, uint attributes, IntPtr template);
    [DllImport("kernel32.dll", SetLastError=true)]
    private static extern bool GetFileSizeEx(IntPtr handle, out long size);
    [DllImport("kernel32.dll", SetLastError=true)]
    private static extern bool CloseHandle(IntPtr handle);

    public static ExportAccessEntry[] Run(string filename) {
        uint[] masks={0x80000000u,0x40000000u,0x10000u,0x80u};
        ExportAccessEntry[] entries=new ExportAccessEntry[masks.Length];
        for(int index=0;index<masks.Length;index++) {
            ExportAccessEntry entry=new ExportAccessEntry();
            entries[index]=entry;entry.desiredAccess=masks[index];
            // OPEN_EXISTING never creates or truncates. These handles only
            // query size. In particular DELETE access does not delete a file.
            IntPtr handle=CreateFileW(filename,masks[index],7,IntPtr.Zero,3,0x80,IntPtr.Zero);
            entry.opened=handle!=new IntPtr(-1);
            if(!entry.opened) {entry.error=Marshal.GetLastWin32Error();continue;}
            try {
                entry.sizeReadable=GetFileSizeEx(handle,out entry.size);
                if(!entry.sizeReadable) entry.sizeError=Marshal.GetLastWin32Error();
            } finally {
                entry.closed=CloseHandle(handle);
                if(!entry.closed) entry.closeError=Marshal.GetLastWin32Error();
            }
        }
        return entries;
    }
}
'@
[ExportFileAccessQuery]::Run($Filename) | ConvertTo-Json -Depth 4 -Compress
