$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false)
Add-Type -TypeDefinition @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class OpenVectorNumericLocale {
    [DllImport("kernel32.dll")]
    private static extern uint GetThreadLocale();
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
    private static extern int GetLocaleInfoW(uint locale, uint kind, StringBuilder value, int count);

    public static int Read(uint kind) {
        uint locale = GetThreadLocale();
        int count = GetLocaleInfoW(locale, kind, null, 0);
        if (count <= 1) throw new InvalidOperationException("Windows returned an empty numeric locale value.");
        var value = new StringBuilder(count);
        if (GetLocaleInfoW(locale, kind, value, count) == 0 || value.Length == 0)
            throw new InvalidOperationException("Windows could not read the numeric locale.");
        // Qt 4.4.3 takes the first QChar, including for the native-digit list.
        return value[0];
    }
}
'@
@{
    decimal = [OpenVectorNumericLocale]::Read(0x0e)
    group = [OpenVectorNumericLocale]::Read(0x0f)
    zero = [OpenVectorNumericLocale]::Read(0x13)
    negative = [OpenVectorNumericLocale]::Read(0x51)
    positive = 43
    exponent = 101
} | ConvertTo-Json -Compress
