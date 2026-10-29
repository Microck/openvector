/* Windows file owner for vector exports.
 *
 * Node's fs always opens with FILE_SHARE_READ|WRITE|DELETE. The original
 * exporters do not: CRT text files deny delete sharing and GDI EMF files deny
 * all sharing. Native captures show both retained handles with granted access
 * 0x120196 (FILE_GENERIC_WRITE|FILE_READ_ATTRIBUTES). This module only opens,
 * writes and closes such handles; buffering and serialization stay in
 * src/vector-file.mjs and the format serializers.
 *
 * The module is built without a C runtime: it links kernel32 and the Node-API
 * symbols exported by node.exe, so it carries no vcruntime dependency. */
#include <windows.h>
#include "node-api/node_api.h"

#define EXPORT_ACCESS (GENERIC_WRITE | FILE_READ_ATTRIBUTES)

static napi_value fail(napi_env env, const char *syscall, DWORD error) {
  /* Map the Win32 errors the exporters can meet onto the codes libuv would
   * report, so callers keep one error vocabulary across platforms. */
  const char *code;
  switch (error) {
    case ERROR_FILE_NOT_FOUND: case ERROR_PATH_NOT_FOUND: case ERROR_INVALID_NAME: code = "ENOENT"; break;
    case ERROR_ACCESS_DENIED: code = "EPERM"; break;
    case ERROR_SHARING_VIOLATION: case ERROR_LOCK_VIOLATION: code = "EBUSY"; break;
    case ERROR_DISK_FULL: case ERROR_HANDLE_DISK_FULL: code = "ENOSPC"; break;
    case ERROR_FILENAME_EXCED_RANGE: code = "ENAMETOOLONG"; break;
    default: code = "EIO"; break;
  }
  /* "<syscall> failed with Windows error <n>", formatted without a CRT. */
  char message[96], digits[12];
  int used = 0, count = 0;
  for (const char *part = syscall; *part; part++) message[used++] = *part;
  for (const char *part = " failed with Windows error "; *part; part++) message[used++] = *part;
  DWORD rest = error;
  do { digits[count++] = (char)('0' + rest % 10); rest /= 10; } while (rest);
  while (count) message[used++] = digits[--count];
  message[used] = 0;
  napi_value codeValue, messageValue, errorValue, numberValue;
  napi_create_string_utf8(env, code, NAPI_AUTO_LENGTH, &codeValue);
  napi_create_string_utf8(env, message, NAPI_AUTO_LENGTH, &messageValue);
  napi_create_error(env, codeValue, messageValue, &errorValue);
  napi_create_uint32(env, error, &numberValue);
  napi_set_named_property(env, errorValue, "winError", numberValue);
  napi_set_named_property(env, errorValue, "syscall", (napi_create_string_utf8(env, syscall, NAPI_AUTO_LENGTH, &messageValue), messageValue));
  napi_throw(env, errorValue);
  return NULL;
}

/* Handles round-trip through a JS number; Win32 handle values stay far
 * below 2^53. A non-number throws instead of becoming a NULL handle. */
static HANDLE readHandle(napi_env env, napi_value value) {
  double number = 0;
  if (napi_get_value_double(env, value, &number) != napi_ok) {
    napi_throw_type_error(env, NULL, "Export handle must be a number.");
    return INVALID_HANDLE_VALUE;
  }
  return (HANDLE)(ULONG_PTR)number;
}

/* open(path, shareMode) -> handle. CREATE_ALWAYS truncates like "w". */
static napi_value openFile(napi_env env, napi_callback_info info) {
  size_t argc = 2;
  napi_value argv[2];
  napi_get_cb_info(env, info, &argc, argv, NULL, NULL);
  size_t length = 0;
  if (napi_get_value_string_utf16(env, argv[0], NULL, 0, &length) != napi_ok) {
    napi_throw_type_error(env, NULL, "Export filename must be a string.");
    return NULL;
  }
  uint32_t share = 0;
  napi_get_value_uint32(env, argv[1], &share);
  HANDLE heap = GetProcessHeap();
  WCHAR *path = HeapAlloc(heap, 0, (length + 1) * sizeof(WCHAR));
  if (path == NULL) return fail(env, "HeapAlloc", ERROR_NOT_ENOUGH_MEMORY);
  napi_get_value_string_utf16(env, argv[0], (char16_t *)path, length + 1, &length);
  HANDLE file = CreateFileW(path, EXPORT_ACCESS, share, NULL, CREATE_ALWAYS, FILE_ATTRIBUTE_NORMAL, NULL);
  DWORD error = GetLastError();
  HeapFree(heap, 0, path);
  if (file == INVALID_HANDLE_VALUE) return fail(env, "CreateFileW", error);
  napi_value result;
  napi_create_double(env, (double)(ULONG_PTR)file, &result);
  return result;
}

/* write(handle, bytes, position). Position -1 continues at the handle's own
 * file pointer, which is what a CRT FILE does after another save truncates
 * the same target. Other positions are explicit physical offsets (EMF). */
static napi_value writeFile(napi_env env, napi_callback_info info) {
  size_t argc = 3;
  napi_value argv[3];
  napi_get_cb_info(env, info, &argc, argv, NULL, NULL);
  HANDLE file = readHandle(env, argv[0]);
  if (file == INVALID_HANDLE_VALUE) return NULL;
  napi_typedarray_type type;
  size_t length = 0;
  void *data = NULL;
  if (napi_get_typedarray_info(env, argv[1], &type, &length, &data, NULL, NULL) != napi_ok || type != napi_uint8_array) {
    napi_throw_type_error(env, NULL, "Export bytes must be a Uint8Array.");
    return NULL;
  }
  int64_t position = -1;
  napi_get_value_int64(env, argv[2], &position);
  const BYTE *bytes = data;
  while (length > 0) {
    DWORD chunk = length > 0x40000000 ? 0x40000000 : (DWORD)length, written = 0;
    OVERLAPPED at;
    OVERLAPPED *target = NULL;
    if (position >= 0) {
      SecureZeroMemory(&at, sizeof at);
      at.Offset = (DWORD)position;
      at.OffsetHigh = (DWORD)(position >> 32);
      target = &at;
    }
    if (!WriteFile(file, bytes, chunk, &written, target)) return fail(env, "WriteFile", GetLastError());
    if (written == 0) return fail(env, "WriteFile", ERROR_WRITE_FAULT);
    bytes += written;
    length -= written;
    if (position >= 0) position += written;
  }
  return NULL;
}

static napi_value closeFile(napi_env env, napi_callback_info info) {
  size_t argc = 1;
  napi_value argv[1];
  napi_get_cb_info(env, info, &argc, argv, NULL, NULL);
  HANDLE file = readHandle(env, argv[0]);
  if (file == INVALID_HANDLE_VALUE) return NULL;
  if (!CloseHandle(file)) return fail(env, "CloseHandle", GetLastError());
  return NULL;
}

NAPI_MODULE_EXPORT int32_t node_api_module_get_api_version_v1(void) { return 8; }

NAPI_MODULE_EXPORT napi_value napi_register_module_v1(napi_env env, napi_value exports) {
  napi_property_descriptor properties[] = {
    {"open", NULL, openFile, NULL, NULL, NULL, napi_enumerable, NULL},
    {"write", NULL, writeFile, NULL, NULL, NULL, napi_enumerable, NULL},
    {"close", NULL, closeFile, NULL, NULL, NULL, napi_enumerable, NULL},
  };
  napi_define_properties(env, exports, 3, properties);
  return exports;
}

/* No CRT: the loader still needs an entry point for a DLL. */
BOOL WINAPI DllEntry(HINSTANCE instance, DWORD reason, LPVOID reserved) {
  (void)instance; (void)reason; (void)reserved;
  return TRUE;
}
