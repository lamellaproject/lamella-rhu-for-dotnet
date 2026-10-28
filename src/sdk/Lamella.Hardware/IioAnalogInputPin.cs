// One analog input pin over the IIO device files the hosting layer presents: the standard pin class, its conversion a
// read of /sys/bus/iio/devices/iio:device0/in_voltage<channel>_raw at the start of the file. The open of the raw file
// claims the channel for this program -- a pad a program holds as a GPIO line refuses it, and a held channel refuses
// the GPIO line -- and the close releases it. The three voltage members compute where AnalogVoltage.IsSupported is
// true and throw where it is false, calling no member of the library the standard class expresses a voltage in, so an
// image built with the switch false carries none of that library's code. This file names that library's namespace:
// the standard class declares its abstract voltage members in the library's quantity type, and an override spells
// the type.
using System;
using System.Device.Analog;
using System.Device.Gpio;
using System.Globalization;
using System.IO;
using System.Text;
using Microsoft.Win32.SafeHandles;
using UnitsNet;

namespace Lamella.Hardware;

internal sealed class IioAnalogInputPin : AnalogInputPin
{
    private const int ErrnoIo = 5;
    private const int ErrnoTimedOut = 110;
    private const int ErrnoBusy = 16;

    private readonly TargetBoard _board;
    private readonly int _channel;
    private readonly int _bits;
    private readonly FileStream _raw;
    private readonly byte[] _text = new byte[32];
    private bool _released;

    private IioAnalogInputPin(IioAnalogController controller, TargetBoard board, int pinNumber, int channel, int bits, FileStream raw)
        : base(controller, pinNumber, controller.VoltageReference)
    {
        _board = board;
        _channel = channel;
        _bits = bits;
        _raw = raw;
    }

    /// <summary>The channel's number on the converter: the number in the raw file's name.</summary>
    internal int Channel => _channel;

    /// <summary>The board's converter reference as a quantity, for the controller's <c>VoltageReference</c> where the switch is true.</summary>
    internal static ElectricPotential ReferenceOf(TargetBoard board) => ElectricPotential.FromMillivolts(board.AnalogReferenceMillivolts);

    internal static IioAnalogInputPin Open(IioAnalogController controller, TargetBoard board, int pinNumber, int channel, int bits)
    {
        string path = IioAnalogController.Device + "/in_voltage" + channel.ToString(CultureInfo.InvariantCulture) + "_raw";
        // The raw file is opened through the stock file classes: the runtime's portability layer presents the device
        // files. The refusal's errno comes back in the exception: a missing file as FileNotFoundException, every other
        // refusal as an IOException whose HResult is the errno.
        SafeFileHandle handle;
        try
        {
            handle = File.OpenHandle(path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite, FileOptions.None);
        }
        catch (FileNotFoundException)
        {
            throw new IOException(NotPresented(board, pinNumber, channel));
        }
        catch (DirectoryNotFoundException)
        {
            throw new IOException(NotPresented(board, pinNumber, channel));
        }
        catch (IOException e) when (e.HResult == ErrnoBusy)
        {
            throw new IOException(Held(board, pinNumber, channel));
        }
        catch (IOException e)
        {
            throw new IOException(OpenRefused(board, path, e.HResult), e);
        }

        FileStream raw;
        try
        {
            // Unbuffered: every read of the class is one read of the file, at the position the class sets.
            raw = new FileStream(handle, FileAccess.Read, bufferSize: 1);
        }
        catch
        {
            handle.Dispose();
            throw;
        }

        return new IioAnalogInputPin(controller, board, pinNumber, channel, bits, raw);
    }

    /// <summary>The width of a conversion in bits: a count lies in 0 to 2^bits - 1.</summary>
    public override int AdcResolutionBits => _bits;

    /// <summary>
    /// One conversion: the count the converter answers, within <see cref="AdcResolutionBits"/>. A conversion that
    /// does not complete within the converter's budget is refused with <see cref="IOException"/> naming the budget (the
    /// converter is reset); a conversion the converter flags in error on every retry is refused with
    /// <see cref="IOException"/> naming the error (the sample is discarded); the next read converts again.
    /// </summary>
    public override uint ReadRaw()
    {
        ThrowIfReleased();
        int n;
        try
        {
            // A read at the start of the raw file is one conversion; the file answers the count as text.
            _raw.Seek(0, SeekOrigin.Begin);
            n = _raw.Read(_text, 0, _text.Length);
        }
        catch (IOException e) when (e.HResult == ErrnoTimedOut)
        {
            throw new IOException(Budget(), e);
        }
        catch (IOException e) when (e.HResult == ErrnoIo)
        {
            throw new IOException(Flagged(), e);
        }

        int end = 0;
        while (end < n && _text[end] >= (byte)'0' && _text[end] <= (byte)'9')
        {
            end++;
        }

        if (end == 0 || end > 9 || (end < n && _text[end] != (byte)'\n'))
        {
            throw new IOException(Where() + " answered '" + Encoding.ASCII.GetString(_text, 0, n).TrimEnd('\n') + "', not a count");
        }

        return uint.Parse(Encoding.ASCII.GetString(_text, 0, end), NumberStyles.None, CultureInfo.InvariantCulture);
    }

    /// <summary>
    /// The smallest voltage a conversion stands for, zero. Where <see cref="AnalogVoltage.IsSupported"/> is false,
    /// refused with <see cref="NotSupportedException"/> naming the board and pointing at <see cref="ReadRaw"/>.
    /// </summary>
    public override ElectricPotential MinVoltage
    {
        get
        {
            if (!AnalogVoltage.IsSupported)
            {
                throw new NotSupportedException(NoVoltage("MinVoltage"));
            }

            // The quantity type's default value is zero in its base unit, volts.
            return default;
        }
    }

    /// <summary>
    /// The voltage a full-scale conversion stands for: the controller's <c>VoltageReference</c> as this pin copied it
    /// at its open (the board's reference, unless the program set another). Where <see cref="AnalogVoltage.IsSupported"/>
    /// is false, refused with <see cref="NotSupportedException"/> naming the board; a reference of zero is refused with
    /// <see cref="InvalidOperationException"/> naming the board's converter reference.
    /// </summary>
    public override ElectricPotential MaxVoltage
    {
        get
        {
            if (!AnalogVoltage.IsSupported)
            {
                throw new NotSupportedException(NoVoltage("MaxVoltage"));
            }

            if (VoltageReference.Value == 0)
            {
                throw new InvalidOperationException(NoReference());
            }

            return VoltageReference;
        }
    }

    /// <summary>
    /// One conversion as a voltage: the standard class's computation, the count over the full scale times
    /// <see cref="MaxVoltage"/>. Where <see cref="AnalogVoltage.IsSupported"/> is false, refused with
    /// <see cref="NotSupportedException"/> naming the board and pointing at <see cref="ReadRaw"/>.
    /// </summary>
    public override ElectricPotential ReadVoltage()
    {
        if (!AnalogVoltage.IsSupported)
        {
            throw new NotSupportedException(NoVoltage("ReadVoltage"));
        }

        return base.ReadVoltage();
    }

    /// <summary>Not supported on this system: the converter reports no change of value; a program reads the input with <see cref="ReadRaw"/>.</summary>
    public override void EnableAnalogValueChangedEvent(GpioController? masterController, int masterPin) =>
        throw new NotSupportedException(NoEvents());

    /// <summary>Not supported on this system, as <see cref="EnableAnalogValueChangedEvent"/> is not.</summary>
    public override void DisableAnalogValueChangedEvent() => throw new NotSupportedException(NoEvents());

    /// <summary>
    /// Releases the channel once (the standard class closes the pin on its controller, which disposes the pin again):
    /// a GPIO line on its pad can be opened again, and the channel opened again.
    /// </summary>
    protected override void Dispose(bool disposing)
    {
        if (disposing && !_released)
        {
            _released = true;
            _raw.Dispose();
        }

        base.Dispose(disposing);
    }

    private void ThrowIfReleased()
    {
        if (_released)
        {
            throw new ObjectDisposedException(nameof(AnalogInputPin), Where() + " was disposed");
        }
    }

    private string Where() => _board.BoardWords() + ": the analog input on " + _board.PinWords(PinNumber) + " (channel " + _channel + ")";

    private string Budget() =>
        Where() + " did not complete the conversion within the converter's budget; the converter was reset, and the next read converts again";

    private string Flagged() =>
        Where() + " refused the conversion: the converter flagged it in error on every retry, so the sample is discarded; the next read converts again";

    private string NoVoltage(string member) =>
        _board.BoardWords() + ": " + member + " is not supported on this board, whose image carries no library for a voltage; ReadRaw answers the count (" + _bits + " bits) and the file " + IioAnalogController.Device + "/in_voltage_scale states the millivolts one count stands for.";

    private string NoReference() =>
        _board.BoardWords() + ": the analog controller's VoltageReference was zero when " + _board.PinWords(PinNumber) + " was opened; the SDK carries the library and the controller sets the reference (" + _board.AnalogReferenceMillivolts + " mV) when the program uses a voltage member, so set the controller's VoltageReference to the board's reference before opening the pin, or leave it as the controller set it.";

    private string NoEvents() => _board.BoardWords() + ": analog value-changed events are not supported on this board; read the input with ReadRaw.";

    private static string Held(TargetBoard board, int pinNumber, int channel)
    {
        string label = board.PinWords(pinNumber);
        return board.BoardWords() + ": the analog input on " + label + " (channel " + channel + ") is held: a program holds " + label + " as a GPIO line, or a controller of this program already holds the channel.";
    }

    private static string NotPresented(TargetBoard board, int pinNumber, int channel) =>
        board.BoardWords() + ": this image presents no analog input on " + board.PinWords(pinNumber) + " (channel " + channel + "), though the board's facts list it";

    private static string OpenRefused(TargetBoard board, string path, int errno) =>
        board.BoardWords() + ": the analog input file " + path + " could not be opened (errno " + errno + ")";
}
