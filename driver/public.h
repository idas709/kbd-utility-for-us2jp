/*++

Module Name:

    public.h

Abstract:

    Shared definitions between the KMDF Keyboard Filter Driver
    and the user-mode Rust service.

Environment:

    Kernel and User mode

--*/

#ifndef _PUBLIC_H_
#define _PUBLIC_H_

#define HID_REMAP_DEVICE_NAME        L"\\Device\\HidKeyboardRemapper"
#define HID_REMAP_SYMBOLIC_NAME      L"\\DosDevices\\HidKeyboardRemapper"
#define HID_REMAP_USERMODE_PATH      L"\\\\.\\HidKeyboardRemapper"

#define FILE_DEVICE_HID_REMAP        0x8000

// IOCTL definitions
#define IOCTL_HID_REMAP_GET_VERSION \
    CTL_CODE(FILE_DEVICE_HID_REMAP, 0x800, METHOD_BUFFERED, FILE_ANY_ACCESS)

#define IOCTL_HID_REMAP_SET_CONFIG \
    CTL_CODE(FILE_DEVICE_HID_REMAP, 0x801, METHOD_BUFFERED, FILE_ANY_ACCESS)

#define IOCTL_HID_REMAP_GET_IME_EVENT \
    CTL_CODE(FILE_DEVICE_HID_REMAP, 0x802, METHOD_BUFFERED, FILE_ANY_ACCESS)

#define IOCTL_HID_REMAP_RESET_STATE \
    CTL_CODE(FILE_DEVICE_HID_REMAP, 0x803, METHOD_BUFFERED, FILE_ANY_ACCESS)

#define DRIVER_VERSION_MAJOR 1
#define DRIVER_VERSION_MINOR 0

#define MAX_INSTANCE_ID_LEN 256
#define MAX_CONFIGURED_DEVICES 16

// Layout modes matching Rust enum
typedef enum _DRIVER_LAYOUT_MODE {
    DRIVER_LAYOUT_PASSTHROUGH = 0,
    DRIVER_LAYOUT_US_TO_JIS   = 1,
    DRIVER_LAYOUT_JIS_TO_US   = 2
} DRIVER_LAYOUT_MODE;

// Configuration for a single keyboard device instance
typedef struct _DRIVER_DEVICE_ENTRY {
    WCHAR InstanceId[MAX_INSTANCE_ID_LEN];
    ULONG LayoutMode;       // DRIVER_LAYOUT_MODE
    BOOLEAN CtrlImeEnabled;
    BOOLEAN Enabled;
    UCHAR Reserved[2];      // Explicit padding
} DRIVER_DEVICE_ENTRY, *PDRIVER_DEVICE_ENTRY;

// Global configuration payload sent from Rust service to driver
typedef struct _DRIVER_GLOBAL_CONFIG {
    BOOLEAN GlobalEnabled;
    UCHAR Reserved[3];      // Explicit padding
    ULONG TapTimeoutMs;
    ULONG DeviceCount;
    DRIVER_DEVICE_ENTRY Devices[MAX_CONFIGURED_DEVICES];
} DRIVER_GLOBAL_CONFIG, *PDRIVER_GLOBAL_CONFIG;

// IME Tap event generated in driver and retrieved by User Session Agent
typedef struct _DRIVER_IME_EVENT {
    WCHAR InstanceId[MAX_INSTANCE_ID_LEN];
    BOOLEAN ImeOn;          // TRUE = IME ON (Right Ctrl), FALSE = IME OFF (Left Ctrl)
    UCHAR Reserved[3];      // Explicit padding
    ULONG TimestampMs;
} DRIVER_IME_EVENT, *PDRIVER_IME_EVENT;

#endif // _PUBLIC_H_
