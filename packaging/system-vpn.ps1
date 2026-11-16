param(
 [ValidateRange(1,65535)][int]$ProxyPort,
 [int]$ParentId,
 [ValidateRange(0,65535)][int]$ControlPort=0,
 [ValidatePattern('^[0-9a-f]{32}$')][string]$Token,
 [switch]$ConfigOnly,
 [switch]$TestRoutes
)
$ErrorActionPreference='Stop'
$name="LiteRustVPN-$ParentId"
$route=@{auto_detect_interface=$true;final='vpn';rules=@(@{process_path=(Join-Path $PSScriptRoot 'xray.exe');action='route';outbound='direct'})}
$tun=@{type='tun';tag='tun';interface_name=$name;address=@('172.29.193.1/30','fdfe:dcba:9876::1/126');mtu=1500;auto_route=$true;strict_route=$true;stack='mixed'}
if($TestRoutes){$tun.route_address=@('198.18.0.0/15');$tun.dns_mode='disabled';$tun.strict_route=$false}
$config=@{log=@{level='error'};dns=@{servers=@(@{type='https';tag='secure-dns';server='1.1.1.1';server_port=443;path='/dns-query';tls=@{enabled=$true;server_name='cloudflare-dns.com'};detour='vpn'});final='secure-dns'};inbounds=@($tun);outbounds=@(@{type='socks';tag='vpn';server='127.0.0.1';server_port=$ProxyPort;version='5'},@{type='direct';tag='direct'});route=$route}|ConvertTo-Json -Depth 20 -Compress
if($ConfigOnly){$config;exit 0}
try {
$parent=Get-Process -Id $ParentId
if([IO.Path]::GetFullPath($parent.Path) -ne [IO.Path]::GetFullPath((Join-Path $PSScriptRoot 'literust-browser.exe'))){throw 'Unexpected parent'}
$connection=[Net.Sockets.TcpClient]::new('127.0.0.1',$ControlPort)
Add-Type @'
using System;using System.Runtime.InteropServices;
public sealed class LiteRustJob:IDisposable{
 [StructLayout(LayoutKind.Sequential)] struct Basic{public long a,b;public uint flags;public UIntPtr min,max;public uint count;public UIntPtr affinity;public uint priority,scheduling;}
 [StructLayout(LayoutKind.Sequential)] struct IO{public ulong a,b,c,d,e,f;}
 [StructLayout(LayoutKind.Sequential)] struct Extended{public Basic basic;public IO io;public UIntPtr a,b,c,d;}
 [DllImport("kernel32.dll",CharSet=CharSet.Unicode)]static extern IntPtr CreateJobObject(IntPtr a,string b);
 [DllImport("kernel32.dll")]static extern bool SetInformationJobObject(IntPtr h,int c,ref Extended e,uint n);
 [DllImport("kernel32.dll")]static extern bool AssignProcessToJobObject(IntPtr h,IntPtr p);
 [DllImport("kernel32.dll")]static extern bool CloseHandle(IntPtr h);
 IntPtr h;public LiteRustJob(){h=CreateJobObject(IntPtr.Zero,null);var e=new Extended();e.basic.flags=0x2000;if(h==IntPtr.Zero||!SetInformationJobObject(h,9,ref e,(uint)Marshal.SizeOf(e)))throw new Exception("Cannot create job");}
 public void Attach(IntPtr p){if(!AssignProcessToJobObject(h,p))throw new Exception("Cannot own engine");}
 public void Dispose(){if(h!=IntPtr.Zero){CloseHandle(h);h=IntPtr.Zero;}}
}
'@
$job=[LiteRustJob]::new();$engine=$null
try{
 $info=[Diagnostics.ProcessStartInfo]::new((Join-Path $PSScriptRoot 'sing-box.exe'),'run -c stdin')
 $info.UseShellExecute=$false;$info.CreateNoWindow=$true;$info.RedirectStandardInput=$true
 $engine=[Diagnostics.Process]::Start($info);$job.Attach($engine.Handle)
 $engine.StandardInput.Write($config);$engine.StandardInput.Close()
 $ready=$false
 for($i=0;$i -lt 40;$i++){
  if($engine.HasExited){throw 'TUN engine stopped'}
  if(Get-NetRoute -InterfaceAlias $name -AddressFamily IPv4 -ErrorAction SilentlyContinue | Where-Object {$_.DestinationPrefix -in @('0.0.0.0/0','0.0.0.0/1','198.18.0.0/15')}){$ready=$true;break}
  Start-Sleep -Milliseconds 250
 }
 if(-not $ready){throw 'TUN interface missing'}
 $writer=[IO.StreamWriter]::new($connection.GetStream());$writer.AutoFlush=$true;$writer.WriteLine("$Token READY")
 while(-not $parent.HasExited -and -not $engine.HasExited){
  if($connection.Client.Poll(0,[Net.Sockets.SelectMode]::SelectRead) -and $connection.Available -eq 0){break}
  Start-Sleep -Milliseconds 250
 }
}finally{
 if($engine -and -not $engine.HasExited){$engine.Kill();$engine.WaitForExit()}
 $job.Dispose()
 Get-NetRoute -InterfaceAlias $name -ErrorAction SilentlyContinue | Remove-NetRoute -Confirm:$false -ErrorAction SilentlyContinue
 $connection.Dispose()
}

} catch { $_.Exception.ToString() | Set-Content (Join-Path $PSScriptRoot "tun-helper-error.log"); exit 1 }
