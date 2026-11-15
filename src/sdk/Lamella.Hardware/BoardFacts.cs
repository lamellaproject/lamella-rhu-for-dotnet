// The facts the SDK generates into a program at publish, handed to TargetBoard before Main.
using System.Collections.Generic;

namespace Lamella.Hardware;

/// <summary>
/// The board's facts as the SDK generated them from the board table's row: the SDK's seam, not a program's.
/// <paramref name="Posture"/> names the image's clock posture in words ("the PLL posture") for the sentences the
/// class prints; <paramref name="AnalogResolutionBits"/> is the width in bits of a conversion on the chip's converter
/// and <paramref name="AnalogReferenceMillivolts"/> the voltage a full-scale conversion stands for, both 0 where the
/// image serves no analog input; <paramref name="SerialPorts"/> is the row's UART blocks with the build's two channel words applied.
/// </summary>
internal sealed record BoardFacts(
    string Name,
    string Maker,
    string Product,
    string Chip,
    int ClockFrequency,
    string Posture,
    IReadOnlyList<BoardPin> Pins,
    IReadOnlyList<BoardI2cBus> I2cBuses,
    IReadOnlyList<BoardSpiBus> SpiBuses,
    IReadOnlyList<UartInfo> SerialPorts,
    int AnalogResolutionBits,
    int AnalogReferenceMillivolts);
