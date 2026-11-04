// Bài 4.2 – Dịch vụ tra cứu thuật ngữ trên UDP (Mã giả 4.3, 4.4; Mục 4.3.1 Giáo trình Lập trình mạng)
// Chạy (trong thư mục bài, nơi có glossary.json):
//        dotnet run -- server [host] [port] [so_datagram_bo_qua]   (mặc định 0.0.0.0 9100 0)
//        dotnet run -- client [host] [port]                        (mặc định 127.0.0.1 9100)
using Bai4_2;

if (args.Length == 0 || (args[0] != "server" && args[0] != "client"))
{
    Console.WriteLine("Cách dùng: dotnet run -- server [host] [port] [so_bo_qua] | client [host] [port]");
    return 1;
}
var rest = args.Skip(1).ToArray();
return args[0] == "server" ? UdpLookupServer.Run(rest) : UdpLookupClient.Run(rest);
