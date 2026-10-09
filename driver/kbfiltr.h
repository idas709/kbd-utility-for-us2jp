/*++

Module Name:

    kbfiltr.h

Abstract:

    Header file for the KMDF Keyboard Filter Driver (Kbdclass Upper Filter).

--*/

#ifndef _KBFILTR_H_
#define _KBFILTR_H_

#include <ntddk.h>
#include <wdf.h>
#include <kbdmou.h>
#include <ntddkbd.h>
#include "public.h"

#define NTDEVICE_NAME_STRING      L"\\Device\\KbFiltr"
#define SYMBOLIC_NAME_STRING      L"\\DosDevices\\KbFiltr"

// Pool tags
#define KBFILTR_POOL_TAG          'tlbK'

// Device extension for each keyboard filter device instance
typedef struct _DEVICE_EXTENSION {
    WDFDEVICE WdfDevice;
    PDEVICE_OBJECT TargetDeviceObject;

    // Device identification
    WCHAR InstanceId[MAX_INSTANCE_ID_LEN];
    BOOLEAN InstanceIdResolved;

    // Hooked kbdclass callback
    CONNECT_DATA UpperConnectData;

    // Per-device configuration
    ULONG LayoutMode;
    BOOLEAN CtrlImeEnabled;

    // Shift and modifier tracking in kernel
    BOOLEAN LShiftPressed;
    BOOLEAN RShiftPressed;
    BOOLEAN LCtrlPressed;
    BOOLEAN RCtrlPressed;
    BOOLEAN LAltPressed;
    BOOLEAN RAltPressed;
    BOOLEAN LWinPressed;
    BOOLEAN RWinPressed;

    // Pending Ctrl tap tracking
    ULONG PendingCtrlType; // 0 = None, 1 = Left, 2 = Right
    ULONG CtrlPressedTimeMs;

    // Active remapped key tracking
    USHORT ActiveTargetScanCode;
    BOOLEAN InjectedShiftDown;
    BOOLEAN InjectedShiftUp;

} DEVICE_EXTENSION, *PDEVICE_EXTENSION;

WDF_DECLARE_CONTEXT_TYPE_WITH_NAME(DEVICE_EXTENSION, FilterGetData)

// Driver Global Context (attached to WDFDRIVER)
typedef struct _DRIVER_CONTEXT {
    WDFDEVICE ControlDevice;
    FAST_MUTEX ConfigMutex;
    DRIVER_GLOBAL_CONFIG CurrentConfig;

    // IME Event Queue (buffered for user-mode agent)
    KSPIN_LOCK EventQueueLock;
    DRIVER_IME_EVENT PendingEvents[32];
    ULONG EventHead;
    ULONG EventTail;
    ULONG EventCount;
} DRIVER_CONTEXT, *PDRIVER_CONTEXT;

WDF_DECLARE_CONTEXT_TYPE_WITH_NAME(DRIVER_CONTEXT, DriverGetContext)

// Function prototypes
DRIVER_INITIALIZE DriverEntry;
EVT_WDF_DRIVER_DEVICE_ADD KbFiltr_EvtDeviceAdd;
EVT_WDF_IO_QUEUE_IO_INTERNAL_DEVICE_CONTROL KbFiltr_EvtIoInternalDeviceControl;
EVT_WDF_IO_QUEUE_IO_DEVICE_CONTROL KbFiltr_ControlDevice_EvtIoDeviceControl;

VOID
KbFiltr_ServiceCallback(
    IN PDEVICE_OBJECT DeviceObject,
    IN PKEYBOARD_INPUT_DATA InputDataStart,
    IN PKEYBOARD_INPUT_DATA InputDataEnd,
    IN OUT PULONG InputDataConsumed
);

NTSTATUS
KbFiltr_CreateControlDevice(
    IN WDFDRIVER Driver
);

VOID
KbFiltr_UpdateDeviceConfig(
    IN PDEVICE_EXTENSION DevExt,
    IN PDRIVER_GLOBAL_CONFIG Config
);

VOID
KbFiltr_EnqueueImeEvent(
    IN PDRIVER_CONTEXT DriverCtx,
    IN PCWSTR InstanceId,
    IN BOOLEAN ImeOn
);

#endif // _KBFILTR_H_
