// One serial port of the board: the chip's UART block, the device file a program opens, the port's lines in this image, and the image's own use of it.
namespace Lamella.Hardware;

/// <summary>
/// One serial port: <paramref name="Number"/> is the chip's UART block number, the number in the port's device
/// file name; <paramref name="PortName"/> is that device file ("/dev/ttyS0" for UART0, "/dev/ttyLP1" for a
/// low-power UART whose block is 1), the name
/// <c>System.IO.Ports.SerialPort</c> takes; <paramref name="Tx"/> and <paramref name="Rx"/> are the pins the port
/// drives in this image, null where the board's facts name no pair for the block; <paramref name="IsConsole"/> and
/// <paramref name="IsLog"/> say that the image routes its console or its kernel log through this port, as the build
/// stated them (the board's default channels, or the project's RhuConsoleChannel and RhuLogChannel words). A port
/// the image uses for either stream is held by the kernel, and a program's open of it is refused.
/// </summary>
public sealed record UartInfo(int Number, string PortName, BoardPin? Tx, BoardPin? Rx, bool IsConsole, bool IsLog);
