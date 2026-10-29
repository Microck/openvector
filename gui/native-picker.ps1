param([ValidateSet('image','files','folder','reveal')][string]$Mode,[string]$InitialDirectory)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false)
Add-Type -TypeDefinition @'
using System;
using System.IO;
using System.Runtime.InteropServices;
namespace OpenVector {
    // Complete inherited vtable order from Windows SDK ShObjIdl_core.h.
    // Uncalled declarations still occupy required COM slots.
    [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)]
    struct Filter { [MarshalAs(UnmanagedType.LPWStr)] public string Name; [MarshalAs(UnmanagedType.LPWStr)] public string Pattern; }
    [ComImport,Guid("d57c7288-d4ad-4768-be02-9d969532d960"),InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    interface IFileOpenDialog {
        [PreserveSig] int Show(IntPtr owner);
        void SetFileTypes(uint count,[MarshalAs(UnmanagedType.LPArray)] Filter[] filters);
        void SetFileTypeIndex(uint index); void GetFileTypeIndex(out uint index);
        void Advise(IntPtr events,out uint cookie); void Unadvise(uint cookie);
        void SetOptions(uint options); void GetOptions(out uint options);
        void SetDefaultFolder(IShellItem item); void SetFolder(IShellItem item);
        void GetFolder(out IShellItem item); void GetCurrentSelection(out IShellItem item);
        void SetFileName([MarshalAs(UnmanagedType.LPWStr)] string name); void GetFileName(out IntPtr name);
        void SetTitle([MarshalAs(UnmanagedType.LPWStr)] string title);
        void SetOkButtonLabel([MarshalAs(UnmanagedType.LPWStr)] string title);
        void SetFileNameLabel([MarshalAs(UnmanagedType.LPWStr)] string title);
        void GetResult(out IShellItem item); void AddPlace(IShellItem item,uint alignment);
        void SetDefaultExtension([MarshalAs(UnmanagedType.LPWStr)] string extension);
        void Close(int status); void SetClientGuid(ref Guid guid); void ClearClientData(); void SetFilter(IntPtr filter);
        void GetResults(out IShellItemArray items); void GetSelectedItems(out IShellItemArray items);
    }
    [ComImport,Guid("43826d1e-e718-42ee-bc55-a1e261c37bfe"),InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    interface IShellItem {
        void BindToHandler(IntPtr context,ref Guid handler,ref Guid iid,out IntPtr value);
        void GetParent(out IShellItem item); void GetDisplayName(uint kind,out IntPtr name);
        void GetAttributes(uint mask,out uint attributes); void Compare(IShellItem item,uint hint,out int order);
    }
    [ComImport,Guid("b63ea76d-1f85-456f-a19c-48159efa858b"),InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    interface IShellItemArray {
        void BindToHandler(IntPtr context,ref Guid handler,ref Guid iid,out IntPtr value);
        void GetPropertyStore(uint flags,ref Guid iid,out IntPtr value);
        void GetPropertyDescriptionList(IntPtr key,ref Guid iid,out IntPtr value);
        void GetAttributes(uint flags,uint mask,out uint attributes);
        void GetCount(out uint count); void GetItemAt(uint index,out IShellItem item); void EnumItems(out IntPtr items);
    }
    public static class NativePicker {
        static string PathOf(IShellItem item) {
            IntPtr name; item.GetDisplayName(0x80058000,out name);
            try {return Marshal.PtrToStringUni(name);} finally {Marshal.FreeCoTaskMem(name);}
        }
        [DllImport("shell32.dll",CharSet=CharSet.Unicode)]
        static extern IntPtr ShellExecute(IntPtr owner,string operation,string file,string parameters,string directory,int show);
        public static void Reveal(string directory) {
            long status = ShellExecute(IntPtr.Zero,"open",directory,null,null,1).ToInt64();
            if (status<=32) throw new IOException("Windows could not open the export folder ("+status+").");
        }
        [DllImport("shell32.dll",CharSet=CharSet.Unicode,PreserveSig=false)]
        static extern void SHCreateItemFromParsingName(string path,IntPtr context,ref Guid iid,out IShellItem item);
        public static string[] Select(string mode,string initialDirectory) {
            var dialog = (IFileOpenDialog)Activator.CreateInstance(Type.GetTypeFromCLSID(new Guid("dc1c5a9c-e88a-4dde-a5a1-60f82a20aef7")));
            try {
                uint options; dialog.GetOptions(out options);
                var application = new Guid("fd55a5d8-2038-4797-aa42-751f20efde4e");
                dialog.SetClientGuid(ref application);
                // Real filesystem paths, existing items, unchanged working directory.
                dialog.SetOptions(options|0x40|0x800|0x1000|0x8|0x2000000|(mode=="folder"?0x20u:mode=="files"?0x200u:0u));
                dialog.SetTitle(mode=="folder"?"OpenVector - Export folder":mode=="image"?"OpenVector - Open image":"OpenVector - Add images");
                if (mode!="folder") dialog.SetFileTypes(2,new [] {
                    new Filter{Name="Bitmap images",Pattern="*.png;*.bmp;*.jpg;*.jpeg;*.jpe;*.gif;*.tif;*.tiff"},
                    new Filter{Name="All files",Pattern="*.*"}});
                // InitialDirectory is a hint; an invalid text-box value must not
                // prevent the user from browsing to an existing export folder.
                if (Directory.Exists(initialDirectory)) {
                    IShellItem folder; var iid = new Guid("43826d1e-e718-42ee-bc55-a1e261c37bfe");
                    SHCreateItemFromParsingName(initialDirectory,IntPtr.Zero,ref iid,out folder);
                    try {dialog.SetFolder(folder);} finally {Marshal.ReleaseComObject(folder);}
                }
                int status = dialog.Show(IntPtr.Zero);
                if (status==unchecked((int)0x800704c7)) return new string[0];
                Marshal.ThrowExceptionForHR(status);
                if (mode=="image") {
                    IShellItem item; dialog.GetResult(out item);
                    try {return new [] {PathOf(item)};} finally {Marshal.ReleaseComObject(item);}
                }
                IShellItemArray items; dialog.GetResults(out items);
                try {
                    uint count; items.GetCount(out count); var paths = new string[count];
                    for (uint index=0;index<count;index++) {
                        IShellItem item; items.GetItemAt(index,out item);
                        try {
                            paths[index]=PathOf(item);
                        } finally {Marshal.ReleaseComObject(item);}
                    }
                    return paths;
                } finally {Marshal.ReleaseComObject(items);}
            } finally {Marshal.ReleaseComObject(dialog);}
        }
    }
}
'@
if ($Mode -eq 'reveal') { [OpenVector.NativePicker]::Reveal($InitialDirectory); $selected = @() }
else { $selected = @([OpenVector.NativePicker]::Select($Mode,$InitialDirectory)) }
ConvertTo-Json -InputObject @($selected) -Compress
