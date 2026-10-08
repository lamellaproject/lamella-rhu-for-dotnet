// The buses a board serves, as the image's facts state them.
using System.Collections.Generic;

namespace Lamella.Hardware;

/// <summary>
/// One I2C bus: its number (the chip's own block number, as Linux names it), its default
/// data and clock pins, and the rates in hertz the image serves at its clock.
/// </summary>
public sealed record I2cBusInfo(int Number, BoardPin Sda, BoardPin Scl, IReadOnlyList<int> Frequencies);

/// <summary>
/// One SPI bus: its number, its default clock, out, in and chip-select pins, and the slowest
/// and fastest rates in hertz the block's ladder makes at the image's clock (a rate asked
/// outside them is served at the nearest end, as a Raspberry Pi serves it).
/// </summary>
public sealed record SpiBusInfo(int Number, BoardPin Sck, BoardPin Mosi, BoardPin Miso, IReadOnlyList<BoardPin> ChipSelects, int MinFrequency, int MaxFrequency);
