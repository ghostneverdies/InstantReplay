using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
namespace InstantReplay.Models;

public sealed record GameRule(string Name, string Executable, string Keyword = "")
{
    public string Description => string.IsNullOrWhiteSpace(Keyword) ? Executable : $"{Executable} · title contains {Keyword}";
}
