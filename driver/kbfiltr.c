/*++

Module Name:

    kbfiltr.c

Abstract:

    KMDF Keyboard Upper Filter Driver implementation.
    Performs per-device scan code translation, modifier tracking,
    and single-tap Ctrl detection.

--*/

#include "kbfiltr.h"

// Forward declaration of internal helper functions
static VOID QueryDeviceInstanceId(PDEVICE_EXTENSION DevExt);

NTSTATUS
DriverEntry(
    IN PDRIVER_OBJECT  DriverObject,
    IN PUNICODE_STRING RegistryPath
)
{
    WDF_DRIVER_CONFIG config;
    NTSTATUS status;
    WDFDRIVER driver;
    WDF_OBJECT_ATTRIBUTES attributes;
    PDRIVER_CONTEXT driverCtx;

    WDF_DRIVER_CONFIG_INIT(&config, KbFiltr_EvtDeviceAdd);

    WDF_OBJECT_ATTRIBUTES_INIT_CONTEXT_TYPE(&attributes, DRIVER_CONTEXT);

    status = WdfDriverCreate(
        DriverObject,
        RegistryPath,
        &attributes,
        &config,
        &driver
    );

    if (!NT_SUCCESS(status)) {
        KdPrint(("KbFiltr: WdfDriverCreate failed 0x%08x\n", status));
        return status;
    }

    driverCtx = DriverGetContext(driver);
    ExInitializeFastMutex(&driverCtx->ConfigMutex);
    KeInitializeSpinLock(&driverCtx->EventQueueLock);
    driverCtx->EventHead = 0;
    driverCtx->EventTail = 0;
    driverCtx->EventCount = 0;
    driverCtx->CurrentConfig.GlobalEnabled = TRUE;
    driverCtx->CurrentConfig.TapTimeoutMs = 1000;
    driverCtx->CurrentConfig.DeviceCount = 0;

    // Create the control device object for user-mode communication
    status = KbFiltr_CreateControlDevice(driver);
    if (!NT_SUCCESS(status)) {
        KdPrint(("KbFiltr: Failed to create control device 0x%08x\n", status));
        // Non-fatal for filtering, but control IOCTLs won't be available
    }

    KdPrint(("KbFiltr: DriverEntry completed successfully.\n"));
    return STATUS_SUCCESS;
}

NTSTATUS
KbFiltr_EvtDeviceAdd(
    IN WDFDRIVER        Driver,
    IN PWDFDEVICE_INIT  DeviceInit
)
{
    WDF_OBJECT_ATTRIBUTES deviceAttributes;
    NTSTATUS status;
    WDFDEVICE hDevice;
    PDEVICE_EXTENSION filterExt;
    WDF_IO_QUEUE_CONFIG ioQueueConfig;

    UNREFERENCED_PARAMETER(Driver);

    // Configure as a filter driver
    WdfFdoInitSetFilter(DeviceInit);

    WDF_OBJECT_ATTRIBUTES_INIT_CONTEXT_TYPE(&deviceAttributes, DEVICE_EXTENSION);

    status = WdfDeviceCreate(&DeviceInit, &deviceAttributes, &hDevice);
    if (!NT_SUCCESS(status)) {
        KdPrint(("KbFiltr: WdfDeviceCreate failed 0x%08x\n", status));
        return status;
    }

    filterExt = FilterGetData(hDevice);
    RtlZeroMemory(filterExt, sizeof(DEVICE_EXTENSION));
    filterExt->WdfDevice = hDevice;
    filterExt->TargetDeviceObject = WdfDeviceWdmGetAttachedDevice(hDevice);

    // Query Device Instance ID from PnP stack
    QueryDeviceInstanceId(filterExt);

    // Configure queue for internal device controls (keyboard connect)
    WDF_IO_QUEUE_CONFIG_INIT_DEFAULT_QUEUE(&ioQueueConfig, WdfIoQueueDispatchParallel);
    ioQueueConfig.EvtIoInternalDeviceControl = KbFiltr_EvtIoInternalDeviceControl;

    status = WdfIoQueueCreate(hDevice, &ioQueueConfig, WDF_NO_OBJECT_ATTRIBUTES, WDF_NO_HANDLE);
    if (!NT_SUCCESS(status)) {
        KdPrint(("KbFiltr: WdfIoQueueCreate failed 0x%08x\n", status));
        return status;
    }

    KdPrint(("KbFiltr: Filter attached to keyboard instance: %ws\n", filterExt->InstanceId));
    return STATUS_SUCCESS;
}

static VOID
QueryDeviceInstanceId(
    IN PDEVICE_EXTENSION DevExt
)
{
    WDFMEMORY memory = NULL;
    NTSTATUS status;
    size_t bufferSize = 0;
    PWSTR idBuffer = NULL;

    WDF_DEVICE_PROPERTY_DATA propertyData;
    WDF_DEVICE_PROPERTY_DATA_INIT(&propertyData, (CONST DEVPROPKEY*)&DEVPKEY_Device_InstanceId);

    status = WdfDeviceAllocAndQueryPropertyEx(
        DevExt->WdfDevice,
        &propertyData,
        NonPagedPoolNx,
        WDF_NO_OBJECT_ATTRIBUTES,
        &memory,
        NULL
    );

    if (NT_SUCCESS(status)) {
        idBuffer = (PWSTR)WdfMemoryGetBuffer(memory, &bufferSize);
        if (idBuffer != NULL && bufferSize > 0) {
            wcsncpy_s(DevExt->InstanceId, MAX_INSTANCE_ID_LEN, idBuffer, _TRUNCATE);
            DevExt->InstanceIdResolved = TRUE;
        }
        WdfObjectDelete(memory);
    } else {
        // Fallback: Empty string, will passthrough by default
        DevExt->InstanceId[0] = L'\0';
        DevExt->InstanceIdResolved = FALSE;
    }
}

VOID
KbFiltr_EvtIoInternalDeviceControl(
    IN WDFQUEUE      Queue,
    IN WDFREQUEST    Request,
    IN size_t        OutputBufferLength,
    IN size_t        InputBufferLength,
    IN ULONG         IoControlCode
)
{
    PDEVICE_EXTENSION devExt = FilterGetData(WdfIoQueueGetDevice(Queue));
    NTSTATUS status = STATUS_SUCCESS;
    WDF_REQUEST_SEND_OPTIONS options;

    UNREFERENCED_PARAMETER(OutputBufferLength);
    UNREFERENCED_PARAMETER(InputBufferLength);

    if (IoControlCode == IOCTL_INTERNAL_KEYBOARD_CONNECT) {
        PCONNECT_DATA connectData = NULL;
        size_t length = 0;

        status = WdfRequestRetrieveInputBuffer(
            Request,
            sizeof(CONNECT_DATA),
            (PVOID*)&connectData,
            &length
        );

        if (NT_SUCCESS(status) && connectData != NULL) {
            if (devExt->UpperConnectData.ClassService == NULL) {
                // Hook the connect data
                devExt->UpperConnectData = *connectData;

                // Hook callback with our filter callback
                connectData->ClassDeviceObject = WdfDeviceWdmGetDeviceObject(devExt->WdfDevice);
                connectData->ClassService = (PVOID)KbFiltr_ServiceCallback;
            } else {
                status = STATUS_SHARING_VIOLATION;
            }
        }
    }

    if (!NT_SUCCESS(status)) {
        WdfRequestComplete(Request, status);
        return;
    }

    // Forward the request down the stack
    WdfRequestFormatRequestUsingCurrentType(Request);
    WDF_REQUEST_SEND_OPTIONS_INIT(&options, WDF_REQUEST_SEND_OPTION_SEND_AND_FORGET);

    if (!WdfRequestSend(Request, WdfDeviceGetIoTarget(devExt->WdfDevice), &options)) {
        status = WdfRequestGetStatus(Request);
        WdfRequestComplete(Request, status);
    }
}

VOID
KbFiltr_ServiceCallback(
    IN PDEVICE_OBJECT       DeviceObject,
    IN PKEYBOARD_INPUT_DATA InputDataStart,
    IN PKEYBOARD_INPUT_DATA InputDataEnd,
    IN OUT PULONG           InputDataConsumed
)
{
    WDFDEVICE wdfDevice = WdfWdmGetDevice(DeviceObject);
    PDEVICE_EXTENSION devExt = FilterGetData(wdfDevice);
    WDFDRIVER driver = WdfGetDriver();
    PDRIVER_CONTEXT driverCtx = DriverGetContext(driver);

    KEYBOARD_INPUT_DATA modifiedBuffer[32];
    ULONG modifiedCount = 0;
    PKEYBOARD_INPUT_DATA current = InputDataStart;

    BOOLEAN globalEnabled = driverCtx->CurrentConfig.GlobalEnabled;

    if (!globalEnabled || devExt->LayoutMode == DRIVER_LAYOUT_PASSTHROUGH) {
        // Direct passthrough to original kbdclass
        (*(PSERVICE_CALLBACK_ROUTINE)devExt->UpperConnectData.ClassService)(
            devExt->UpperConnectData.ClassDeviceObject,
            InputDataStart,
            InputDataEnd,
            InputDataConsumed
        );
        return;
    }

    // Process each input data item
    while (current < InputDataEnd && modifiedCount < 28) {
        USHORT makeCode = current->MakeCode;
        USHORT flags = current->Flags;
        BOOLEAN isUp = (flags & KEY_BREAK) != 0;
        BOOLEAN isE0 = (flags & KEY_E0) != 0;

        // Modifier key tracking
        if (makeCode == 0x2A && !isE0) devExt->LShiftPressed = !isUp;
        if (makeCode == 0x36 && !isE0) devExt->RShiftPressed = !isUp;
        if (makeCode == 0x1D && !isE0) devExt->LCtrlPressed = !isUp;
        if (makeCode == 0x1D && isE0)  devExt->RCtrlPressed = !isUp;
        if (makeCode == 0x38 && !isE0) devExt->LAltPressed = !isUp;
        if (makeCode == 0x38 && isE0)  devExt->RAltPressed = !isUp;

        // Single-tap Ctrl tracking
        if (devExt->CtrlImeEnabled) {
            LARGE_INTEGER tickCount;
            KeQueryTickCount(&tickCount);
            ULONG timeMs = (ULONG)(tickCount.QuadPart * KeQueryTimeIncrement() / 10000);

            if (makeCode == 0x1D) {
                if (!isUp) {
                    // Down
                    devExt->PendingCtrlType = isE0 ? 2 : 1; // 1 = Left, 2 = Right
                    devExt->CtrlPressedTimeMs = timeMs;
                } else {
                    // Up: check timeout
                    if (devExt->PendingCtrlType != 0) {
                        ULONG elapsed = timeMs - devExt->CtrlPressedTimeMs;
                        if (elapsed <= driverCtx->CurrentConfig.TapTimeoutMs) {
                            BOOLEAN imeOn = (devExt->PendingCtrlType == 2);
                            KbFiltr_EnqueueImeEvent(driverCtx, devExt->InstanceId, imeOn);
                        }
                    }
                    devExt->PendingCtrlType = 0;
                }
            } else {
                // Any other key press cancels Ctrl tap
                if (!isUp) {
                    devExt->PendingCtrlType = 0;
                }
            }
        }

        // Layout transformation (US -> JIS)
        if (devExt->LayoutMode == DRIVER_LAYOUT_US_TO_JIS &&
            !devExt->LCtrlPressed && !devExt->RCtrlPressed &&
            !devExt->LAltPressed && !devExt->RAltPressed)
        {
            BOOLEAN shiftHeld = devExt->LShiftPressed || devExt->RShiftPressed;

            // Example: US Shift + 2 (@) -> JIS unshifted 0x1A
            if (makeCode == 0x03 && shiftHeld) { // '2'
                if (!isUp) {
                    // Inject Shift Up, then JIS @ Down
                    modifiedBuffer[modifiedCount] = *current;
                    modifiedBuffer[modifiedCount].MakeCode = 0x2A;
                    modifiedBuffer[modifiedCount].Flags = KEY_BREAK;
                    modifiedCount++;

                    modifiedBuffer[modifiedCount] = *current;
                    modifiedBuffer[modifiedCount].MakeCode = 0x1A;
                    modifiedBuffer[modifiedCount].Flags = KEY_MAKE;
                    modifiedCount++;
                    devExt->InjectedShiftUp = TRUE;
                } else {
                    // JIS @ Up, then restore Shift Down
                    modifiedBuffer[modifiedCount] = *current;
                    modifiedBuffer[modifiedCount].MakeCode = 0x1A;
                    modifiedBuffer[modifiedCount].Flags = KEY_BREAK;
                    modifiedCount++;

                    modifiedBuffer[modifiedCount] = *current;
                    modifiedBuffer[modifiedCount].MakeCode = 0x2A;
                    modifiedBuffer[modifiedCount].Flags = KEY_MAKE;
                    modifiedCount++;
                    devExt->InjectedShiftUp = FALSE;
                }
                current++;
                continue;
            }

            // Example: US unshifted ''' (0x28) -> JIS Shift + 7 (0x08)
            if (makeCode == 0x28 && !shiftHeld) {
                if (!isUp) {
                    // Inject Shift Down, then 7 Down
                    modifiedBuffer[modifiedCount] = *current;
                    modifiedBuffer[modifiedCount].MakeCode = 0x2A;
                    modifiedBuffer[modifiedCount].Flags = KEY_MAKE;
                    modifiedCount++;

                    modifiedBuffer[modifiedCount] = *current;
                    modifiedBuffer[modifiedCount].MakeCode = 0x08;
                    modifiedBuffer[modifiedCount].Flags = KEY_MAKE;
                    modifiedCount++;
                    devExt->InjectedShiftDown = TRUE;
                } else {
                    // 7 Up, then Shift Up
                    modifiedBuffer[modifiedCount] = *current;
                    modifiedBuffer[modifiedCount].MakeCode = 0x08;
                    modifiedBuffer[modifiedCount].Flags = KEY_BREAK;
                    modifiedCount++;

                    modifiedBuffer[modifiedCount] = *current;
                    modifiedBuffer[modifiedCount].MakeCode = 0x2A;
                    modifiedBuffer[modifiedCount].Flags = KEY_BREAK;
                    modifiedCount++;
                    devExt->InjectedShiftDown = FALSE;
                }
                current++;
                continue;
            }
        }

        // Passthrough current key unchanged
        modifiedBuffer[modifiedCount++] = *current;
        current++;
    }

    // Deliver modified events to upper kbdclass
    if (modifiedCount > 0) {
        ULONG consumed = 0;
        (*(PSERVICE_CALLBACK_ROUTINE)devExt->UpperConnectData.ClassService)(
            devExt->UpperConnectData.ClassDeviceObject,
            &modifiedBuffer[0],
            &modifiedBuffer[modifiedCount],
            &consumed
        );
    }

    *InputDataConsumed = (ULONG)(current - InputDataStart);
}

VOID
KbFiltr_EnqueueImeEvent(
    IN PDRIVER_CONTEXT DriverCtx,
    IN PCWSTR          InstanceId,
    IN BOOLEAN         ImeOn
)
{
    KLOCK_QUEUE_HANDLE lockHandle;
    KeAcquireInStackQueuedSpinLock(&DriverCtx->EventQueueLock, &lockHandle);

    if (DriverCtx->EventCount < 32) {
        PDRIVER_IME_EVENT evt = &DriverCtx->PendingEvents[DriverCtx->EventTail];
        wcsncpy_s(evt->InstanceId, MAX_INSTANCE_ID_LEN, InstanceId, _TRUNCATE);
        evt->ImeOn = ImeOn;

        LARGE_INTEGER tickCount;
        KeQueryTickCount(&tickCount);
        evt->TimestampMs = (ULONG)(tickCount.QuadPart * KeQueryTimeIncrement() / 10000);

        DriverCtx->EventTail = (DriverCtx->EventTail + 1) % 32;
        DriverCtx->EventCount++;
    }

    KeReleaseInStackQueuedSpinLock(&lockHandle);
}

NTSTATUS
KbFiltr_CreateControlDevice(
    IN WDFDRIVER Driver
)
{
    PWDFDEVICE_INIT deviceInit = NULL;
    WDF_OBJECT_ATTRIBUTES attributes;
    WDFDEVICE controlDevice;
    PDRIVER_CONTEXT driverCtx = DriverGetContext(Driver);
    UNICODE_STRING deviceName;
    UNICODE_STRING symbolicName;
    WDF_IO_QUEUE_CONFIG ioQueueConfig;
    NTSTATUS status;

    deviceInit = WdfControlDeviceInitAllocate(Driver, &SDDL_DEVPARAMS_SYS_ALL_ADM_RWX_WORLD_R_RES_R);
    if (!deviceInit) {
        return STATUS_INSUFFICIENT_RESOURCES;
    }

    RtlInitUnicodeString(&deviceName, HID_REMAP_DEVICE_NAME);
    status = WdfDeviceInitAssignName(deviceInit, &deviceName);
    if (!NT_SUCCESS(status)) {
        WdfDeviceInitFree(deviceInit);
        return status;
    }

    WDF_OBJECT_ATTRIBUTES_INIT(&attributes);
    status = WdfDeviceCreate(&deviceInit, &attributes, &controlDevice);
    if (!NT_SUCCESS(status)) {
        WdfDeviceInitFree(deviceInit);
        return status;
    }

    RtlInitUnicodeString(&symbolicName, HID_REMAP_SYMBOLIC_NAME);
    status = WdfDeviceCreateSymbolicLink(controlDevice, &symbolicName);
    if (!NT_SUCCESS(status)) {
        WdfObjectDelete(controlDevice);
        return status;
    }

    WDF_IO_QUEUE_CONFIG_INIT_DEFAULT_QUEUE(&ioQueueConfig, WdfIoQueueDispatchSequential);
    ioQueueConfig.EvtIoDeviceControl = KbFiltr_ControlDevice_EvtIoDeviceControl;

    status = WdfIoQueueCreate(controlDevice, &ioQueueConfig, WDF_NO_OBJECT_ATTRIBUTES, WDF_NO_HANDLE);
    if (!NT_SUCCESS(status)) {
        WdfObjectDelete(controlDevice);
        return status;
    }

    WdfControlFinishInitializing(controlDevice);
    driverCtx->ControlDevice = controlDevice;

    return STATUS_SUCCESS;
}

VOID
KbFiltr_ControlDevice_EvtIoDeviceControl(
    IN WDFQUEUE   Queue,
    IN WDFREQUEST Request,
    IN size_t     OutputBufferLength,
    IN size_t     InputBufferLength,
    IN ULONG      IoControlCode
)
{
    WDFDEVICE device = WdfIoQueueGetDevice(Queue);
    WDFDRIVER driver = WdfDeviceGetDriver(device);
    PDRIVER_CONTEXT driverCtx = DriverGetContext(driver);
    NTSTATUS status = STATUS_SUCCESS;
    size_t bytesReturned = 0;

    UNREFERENCED_PARAMETER(OutputBufferLength);
    UNREFERENCED_PARAMETER(InputBufferLength);

    switch (IoControlCode) {
    case IOCTL_HID_REMAP_GET_VERSION: {
        ULONG* pVersion = NULL;
        status = WdfRequestRetrieveOutputBuffer(Request, sizeof(ULONG), (PVOID*)&pVersion, NULL);
        if (NT_SUCCESS(status) && pVersion) {
            *pVersion = (DRIVER_VERSION_MAJOR << 16) | DRIVER_VERSION_MINOR;
            bytesReturned = sizeof(ULONG);
        }
        break;
    }

    case IOCTL_HID_REMAP_SET_CONFIG: {
        PDRIVER_GLOBAL_CONFIG pConfig = NULL;
        status = WdfRequestRetrieveInputBuffer(Request, sizeof(DRIVER_GLOBAL_CONFIG), (PVOID*)&pConfig, NULL);
        if (NT_SUCCESS(status) && pConfig) {
            ExAcquireFastMutex(&driverCtx->ConfigMutex);
            RtlCopyMemory(&driverCtx->CurrentConfig, pConfig, sizeof(DRIVER_GLOBAL_CONFIG));
            ExReleaseFastMutex(&driverCtx->ConfigMutex);
            bytesReturned = 0;
        }
        break;
    }

    case IOCTL_HID_REMAP_GET_IME_EVENT: {
        PDRIVER_IME_EVENT pOutEvent = NULL;
        status = WdfRequestRetrieveOutputBuffer(Request, sizeof(DRIVER_IME_EVENT), (PVOID*)&pOutEvent, NULL);
        if (NT_SUCCESS(status) && pOutEvent) {
            KLOCK_QUEUE_HANDLE lockHandle;
            KeAcquireInStackQueuedSpinLock(&driverCtx->EventQueueLock, &lockHandle);

            if (driverCtx->EventCount > 0) {
                *pOutEvent = driverCtx->PendingEvents[driverCtx->EventHead];
                driverCtx->EventHead = (driverCtx->EventHead + 1) % 32;
                driverCtx->EventCount--;
                bytesReturned = sizeof(DRIVER_IME_EVENT);
                status = STATUS_SUCCESS;
            } else {
                status = STATUS_NO_MORE_ENTRIES;
            }

            KeReleaseInStackQueuedSpinLock(&lockHandle);
        }
        break;
    }

    default:
        status = STATUS_INVALID_DEVICE_REQUEST;
        break;
    }

    WdfRequestCompleteWithInformation(Request, status, bytesReturned);
}
