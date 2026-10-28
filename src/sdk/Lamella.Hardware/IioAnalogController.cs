// The analog controller over the IIO device files the hosting layer presents: the standard controller class with the
// board's facts behind its abstract members. The pins are numbered by GPIO line, the numbers GpioController.OpenPin
// takes; a line's analog input is the channel its record carries on the chip's converter, the one device
// /sys/bus/iio/devices/iio:device0. The standard class keeps the open pins by number and refuses a second open of a
// pin it holds; under it, the open of the channel's raw file claims the channel for this program, so a pad held as a
// GPIO line, or by another controller, is refused at the open. Where AnalogVoltage.IsSupported is true the
// controller's VoltageReference is set to the board's reference at its construction, the standard Arduino controller's
// way, and a program may set another before it opens a pin; where it is false the reference stays unset and the voltage
// members throw. The pin's copy, taken at its open, is what its voltage members use.
using System;
using System.Device.Analog;
using System.IO;

namespace Lamella.Hardware;

internal sealed class IioAnalogController : AnalogController
{
    /// <summary>The one converter's device directory.</summary>
    internal const string Device = "/sys/bus/iio/devices/iio:device0";

    private readonly TargetBoard _board;

    internal IioAnalogController(TargetBoard board)
    {
        _board = board;
        if (AnalogVoltage.IsSupported)
        {
            // The board's reference as a quantity; the branch leaves an image built with the switch false.
            VoltageReference = IioAnalogInputPin.ReferenceOf(board);
        }
    }

    /// <summary>
    /// The number of pins the controller numbers: the chip's package, the range of the numbers
    /// <see cref="AnalogController.OpenPin"/> takes; <see cref="SupportsAnalogInput"/> says which carry an analog input.
    /// </summary>
    public override int PinCount => _board.Pins.Count;

    /// <summary>Whether the pin carries an analog input function on this board.</summary>
    public override bool SupportsAnalogInput(int pin) =>
        pin >= 0 && pin < _board.Pins.Count && _board.Pins[pin].AdcChannel is not null;

    /// <summary>The channel number of the pin's analog input on the converter, or -1 where the pin carries none.</summary>
    public override int ConvertPinNumberToAnalogChannelNumber(int pinNumber) =>
        pinNumber >= 0 && pinNumber < _board.Pins.Count && _board.Pins[pinNumber].AdcChannel is int channel ? channel : -1;

    /// <summary>The pin whose analog input is the channel, or -1 where no pin reads it (the chip's own sources).</summary>
    public override int ConvertAnalogChannelNumberToPinNumber(int analogChannelNumber)
    {
        foreach (BoardPin pin in _board.Pins)
        {
            if (pin.AdcChannel == analogChannelNumber)
            {
                return pin.PinNumber;
            }
        }

        return -1;
    }

    /// <summary>
    /// The pin over its channel's raw file, claimed for this program: a channel held already (through a GPIO line a
    /// program holds on the pad, or by another controller of this program) is refused with <see cref="IOException"/>
    /// naming the pad. The standard class refuses a pin the board does not support, and a pin this controller holds,
    /// before this is called.
    /// </summary>
    protected override AnalogInputPin OpenPinCore(int pinNumber)
    {
        if (_board.Pins[pinNumber].AdcChannel is not int channel)
        {
            throw new NotSupportedException(_board.BoardWords() + ": " + _board.PinWords(pinNumber) + " carries no analog input function on this board.");
        }

        return IioAnalogInputPin.Open(this, _board, pinNumber, channel, _board.AnalogResolutionBits);
    }
}
