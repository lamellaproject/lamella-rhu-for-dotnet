// One record per pin of the chip's package: the number GpioController.OpenPin takes, the labels the board prints for it, and what the pin can be.
using System;
using System.Collections.Generic;

namespace Lamella.Hardware;

/// <summary>What a pin can be, by the chip's own function table.</summary>
[Flags]
public enum PinFunctions
{
    None = 0,
    Gpio = 1,
    I2cSda = 2,
    I2cScl = 4,
    SpiSck = 8,
    SpiMosi = 16,
    SpiMiso = 32,
    SpiCs = 64,
    UartTx = 128,
    UartRx = 256,
    Pwm = 512,
    Adc = 1024,
}

/// <summary>
/// One pin of the chip's package as the board presents it: <paramref name="PinNumber"/> is the
/// number <c>GpioController.OpenPin</c> takes; <paramref name="Labels"/> are the names printed
/// on the board for it ("GP25" on a Pico, "PA5" on an ST board), empty where the board prints
/// none; <paramref name="Functions"/> is what the pin can be, with <paramref name="I2cBus"/> and
/// <paramref name="SpiBus"/> the block numbers those functions belong to.
/// </summary>
public sealed record BoardPin(int PinNumber, IReadOnlyList<string> Labels, PinFunctions Functions, int? I2cBus, int? SpiBus);
