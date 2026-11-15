// The buses a board serves, as the image's facts state them.
using System.Collections.Generic;

namespace Lamella.Hardware;

/// <summary>
/// One I2C bus: its number (the chip's own block number, as Linux names it), its default
/// data and clock pins, and the rates in hertz the image serves at its clock.
/// </summary>
public sealed record BoardI2cBus(int Number, BoardPin Sda, BoardPin Scl, IReadOnlyList<int> Frequencies);

/// <summary>
/// One SPI bus: its number, its default clock, out, in and chip-select pins, and the slowest
/// and fastest rates in hertz the block's ladder makes at the image's clock (a rate asked
/// above the fastest is served at the fastest; a rate asked below the slowest is refused).
/// </summary>
public sealed record BoardSpiBus(int Number, BoardPin Sck, BoardPin Mosi, BoardPin Miso, IReadOnlyList<BoardPin> ChipSelects, int MinFrequency, int MaxFrequency);
