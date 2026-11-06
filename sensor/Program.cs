using System.Text.Json;
using LibreHardwareMonitor.Hardware;

var computer = new Computer
{
    IsCpuEnabled = true,
    IsGpuEnabled = true,
};

try
{
    computer.Open();
    while (true)
    {
        float? cpu = null;
        float? gpu = null;
        foreach (var hardware in computer.Hardware)
            Sample(hardware, ref cpu, ref gpu);

        Console.WriteLine(JsonSerializer.Serialize(new { cpu_temp = cpu, gpu_temp = gpu }));
        Console.Out.Flush();
        Thread.Sleep(1000);
    }
}
finally
{
    computer.Close();
}

static void Sample(IHardware hardware, ref float? cpu, ref float? gpu)
{
    hardware.Update();
    foreach (var sensor in hardware.Sensors)
    {
        if (sensor.SensorType != SensorType.Temperature || sensor.Value is not float value ||
            !float.IsFinite(value) || value is < -20 or > 150)
            continue;

        if (hardware.HardwareType == HardwareType.Cpu)
            cpu = cpu is null ? value : Math.Max(cpu.Value, value);
        else if (hardware.HardwareType is HardwareType.GpuNvidia or HardwareType.GpuAmd)
            gpu = gpu is null ? value : Math.Max(gpu.Value, value);
    }

    foreach (var child in hardware.SubHardware)
        Sample(child, ref cpu, ref gpu);
}
