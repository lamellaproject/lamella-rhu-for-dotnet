/* The refused entry points of the .NET runtime pack's P/Invoke-facing native
 * surface at the pin: one function per exported name the shim's hand-written
 * units do not define, each answering the defined failure of its family --
 * the PAL's own error encoding, or -1 with the errno of record, or the
 * library's own failure value -- nothing printed, nothing trapped. What is
 * refused and why is the shim's not-served table (the SDK's document): the
 * sockets and the network readers (no network stack in this image), the
 * process family (one process, no fork, exec or signal to another), the
 * file-system features the store lacks (links, nodes, watches, pipe sizes),
 * the ICU-backed globalization calls (the image's culture is invariant), the
 * compression, GSSAPI and OpenSSL-backed libraries (not carried). GENERATED
 * by tools/extract-pal/gen_pal_surface.py at tag v11.0.0-rc.1.26425.128
 * (commit ab19415702aa8139d5369e47c73edb47343c34ad); regenerate at a re-pin, never edit by hand. */
#include <stddef.h>
#include <stdint.h>

#include "rhu_glibc_arm32.h"
#include "rhu_pal_errno_net11_rc1.h"
#include "rhu_pal_surface_net11_rc1.h"

extern int *__errno_location(void);
#define ERRNO (*__errno_location())

/* ------------------------------------------------------------------ */
/* SystemNative_ */
/* ------------------------------------------------------------------ */

int32_t SystemNative_Accept(intptr_t socket, uint8_t* socketAddress, int32_t* socketAddressLen, intptr_t* acceptedSocket)
{
    (void)socket;
    (void)socketAddress;
    (void)socketAddressLen;
    (void)acceptedSocket;
    return RHU_PAL_ENOTSUP;
}

int32_t SystemNative_Bind(intptr_t socket, int32_t protocolType, uint8_t* socketAddress, int32_t socketAddressLen)
{
    (void)socket;
    (void)protocolType;
    (void)socketAddress;
    (void)socketAddressLen;
    return RHU_PAL_ENOTSUP;
}

int32_t SystemNative_CloseSocketEventPort(intptr_t port)
{
    (void)port;
    return RHU_PAL_ENOTSUP;
}

int32_t SystemNative_Connect(intptr_t socket, uint8_t* socketAddress, int32_t socketAddressLen)
{
    (void)socket;
    (void)socketAddress;
    (void)socketAddressLen;
    return RHU_PAL_ENOTSUP;
}

int32_t SystemNative_Connectx(intptr_t socket, uint8_t* socketAddress, int32_t socketAddressLen, uint8_t* data, int32_t dataLen, int32_t tfo, int* sent)
{
    (void)socket;
    (void)socketAddress;
    (void)socketAddressLen;
    (void)data;
    (void)dataLen;
    (void)tfo;
    (void)sent;
    return RHU_PAL_ENOTSUP;
}

int32_t SystemNative_CreateNetworkChangeListenerSocket(intptr_t* retSocket)
{
    (void)retSocket;
    return RHU_PAL_ENOTSUP;
}

int32_t SystemNative_CreateSocketEventBuffer(int32_t count, SocketEvent** buffer)
{
    (void)count;
    (void)buffer;
    return RHU_PAL_ENOTSUP;
}

int32_t SystemNative_CreateSocketEventPort(intptr_t* port)
{
    (void)port;
    return RHU_PAL_ENOTSUP;
}

int32_t SystemNative_Disconnect(intptr_t socket)
{
    (void)socket;
    return RHU_PAL_ENOTSUP;
}

int32_t SystemNative_EnumerateGatewayAddressesForInterface(void* context, uint32_t interfaceIndex, GatewayAddressFound onGatewayFound)
{
    (void)context;
    (void)interfaceIndex;
    (void)onGatewayFound;
    ERRNO = RHU_GERR_ENOTSUP;
    return -1;
}

int32_t SystemNative_EnumerateInterfaceAddresses(void* context, IPv4AddressFound onIpv4Found, IPv6AddressFound onIpv6Found, LinkLayerAddressFound onLinkLayerFound)
{
    (void)context;
    (void)onIpv4Found;
    (void)onIpv6Found;
    (void)onLinkLayerFound;
    ERRNO = RHU_GERR_ENOTSUP;
    return -1;
}

int32_t SystemNative_FChflags(intptr_t fd, uint32_t flags)
{
    (void)fd;
    (void)flags;
    ERRNO = RHU_GERR_ENOTSUP;
    return -1;
}

int32_t SystemNative_FcntlGetPipeSz(intptr_t fd)
{
    (void)fd;
    ERRNO = RHU_GERR_ENOTSUP;
    return -1;
}

int32_t SystemNative_FcntlSetPipeSz(intptr_t fd, int32_t size)
{
    (void)fd;
    (void)size;
    ERRNO = RHU_GERR_ENOTSUP;
    return -1;
}

int32_t SystemNative_ForkAndExecProcess(const char* filename, char* const argv[], char* const envp[], const char* cwd, int32_t setCredentials, uint32_t userId, uint32_t groupId, uint32_t* groups, int32_t groupsLength, int32_t* childPid, int32_t stdinFd, int32_t stdoutFd, int32_t stderrFd, int32_t* inheritedFds, int32_t inheritedFdCount, int32_t startDetached, int32_t killOnParentExit, int32_t startSuspended)
{
    (void)filename;
    (void)argv;
    (void)envp;
    (void)cwd;
    (void)setCredentials;
    (void)userId;
    (void)groupId;
    (void)groups;
    (void)groupsLength;
    (void)childPid;
    (void)stdinFd;
    (void)stdoutFd;
    (void)stderrFd;
    (void)inheritedFds;
    (void)inheritedFdCount;
    (void)startDetached;
    (void)killOnParentExit;
    (void)startSuspended;
    ERRNO = RHU_GERR_ENOSYS;
    return -1;
}

void SystemNative_FreeHostEntry(HostEntry* entry)
{
    (void)entry;
}

int32_t SystemNative_FreeSocketEventBuffer(SocketEvent* buffer)
{
    (void)buffer;
    return RHU_PAL_ENOTSUP;
}

int32_t SystemNative_GetActiveTcpConnectionInfos(NativeTcpConnectionInformation* infos, int32_t* infoCount)
{
    (void)infos;
    (void)infoCount;
    ERRNO = RHU_GERR_ENOTSUP;
    return -1;
}

int32_t SystemNative_GetActiveUdpListeners(IPEndPointInfo* infos, int32_t* infoCount)
{
    (void)infos;
    (void)infoCount;
    ERRNO = RHU_GERR_ENOTSUP;
    return -1;
}

int32_t SystemNative_GetAddressFamily(const uint8_t* socketAddress, int32_t socketAddressLen, int32_t* addressFamily)
{
    (void)socketAddress;
    (void)socketAddressLen;
    (void)addressFamily;
    return RHU_PAL_ENOTSUP;
}

int32_t SystemNative_GetAtOutOfBandMark(intptr_t socket, int32_t* available)
{
    (void)socket;
    (void)available;
    return RHU_PAL_ENOTSUP;
}

int32_t SystemNative_GetBytesAvailable(intptr_t socket, int32_t* available)
{
    (void)socket;
    (void)available;
    return RHU_PAL_ENOTSUP;
}

int32_t SystemNative_GetControlMessageBufferSize(int32_t isIPv4, int32_t isIPv6)
{
    (void)isIPv4;
    (void)isIPv6;
    return 0;
}

void SystemNative_GetDomainSocketSizes(int32_t* pathOffset, int32_t* pathSize, int32_t* addressSize)
{
    (void)pathOffset;
    (void)pathSize;
    (void)addressSize;
}

int32_t SystemNative_GetEstimatedTcpConnectionCount(void)
{
    ERRNO = RHU_GERR_ENOTSUP;
    return -1;
}

int32_t SystemNative_GetEstimatedUdpListenerCount(void)
{
    ERRNO = RHU_GERR_ENOTSUP;
    return -1;
}

int32_t SystemNative_GetHostEntryForName(const uint8_t* address, int32_t addressFamily, HostEntry* entry)
{
    (void)address;
    (void)addressFamily;
    (void)entry;
    return RHU_PAL_GetAddrInfoErrorFlags_EAI_FAIL;
}

int32_t SystemNative_GetIPv4Address(const uint8_t* socketAddress, int32_t socketAddressLen, uint32_t* address)
{
    (void)socketAddress;
    (void)socketAddressLen;
    (void)address;
    return RHU_PAL_ENOTSUP;
}

int32_t SystemNative_GetIPv4GlobalStatistics(IPv4GlobalStatistics* retStats)
{
    (void)retStats;
    ERRNO = RHU_GERR_ENOTSUP;
    return -1;
}

int32_t SystemNative_GetIPv4MulticastOption(intptr_t socket, int32_t multicastOption, IPv4MulticastOption* option)
{
    (void)socket;
    (void)multicastOption;
    (void)option;
    return RHU_PAL_ENOTSUP;
}

int32_t SystemNative_GetIPv6Address(const uint8_t* socketAddress, int32_t socketAddressLen, uint8_t* address, int32_t addressLen, uint32_t* scopeId)
{
    (void)socketAddress;
    (void)socketAddressLen;
    (void)address;
    (void)addressLen;
    (void)scopeId;
    return RHU_PAL_ENOTSUP;
}

int32_t SystemNative_GetIPv6MulticastOption(intptr_t socket, int32_t multicastOption, IPv6MulticastOption* option)
{
    (void)socket;
    (void)multicastOption;
    (void)option;
    return RHU_PAL_ENOTSUP;
}

int32_t SystemNative_GetIcmpv4GlobalStatistics(Icmpv4GlobalStatistics* retStats)
{
    (void)retStats;
    ERRNO = RHU_GERR_ENOTSUP;
    return -1;
}

int32_t SystemNative_GetIcmpv6GlobalStatistics(Icmpv6GlobalStatistics* retStats)
{
    (void)retStats;
    ERRNO = RHU_GERR_ENOTSUP;
    return -1;
}

int32_t SystemNative_GetLingerOption(intptr_t socket, LingerOption* option)
{
    (void)socket;
    (void)option;
    return RHU_PAL_ENOTSUP;
}

int32_t SystemNative_GetMaximumAddressSize(void)
{
    return 0;
}

int32_t SystemNative_GetNameInfo(const uint8_t* address, int32_t addressLength, int8_t isIPv6, uint8_t* host, int32_t hostLength, uint8_t* service, int32_t serviceLength, int32_t flags)
{
    (void)address;
    (void)addressLength;
    (void)isIPv6;
    (void)host;
    (void)hostLength;
    (void)service;
    (void)serviceLength;
    (void)flags;
    return RHU_PAL_GetAddrInfoErrorFlags_EAI_FAIL;
}

int32_t SystemNative_GetNativeIPInterfaceStatistics(char* interfaceName, NativeIPInterfaceStatistics* retStats)
{
    (void)interfaceName;
    (void)retStats;
    ERRNO = RHU_GERR_ENOTSUP;
    return -1;
}

int32_t SystemNative_GetNetworkInterfaces(int32_t* interfaceCount, NetworkInterfaceInfo** interfaces, int32_t* addressCount, IpAddressInfo** addressList)
{
    (void)interfaceCount;
    (void)interfaces;
    (void)addressCount;
    (void)addressList;
    ERRNO = RHU_GERR_ENOTSUP;
    return -1;
}

int32_t SystemNative_GetNumRoutes(void)
{
    ERRNO = RHU_GERR_ENOTSUP;
    return -1;
}

int32_t SystemNative_GetPeerID(intptr_t socket, uint32_t* euid)
{
    (void)socket;
    (void)euid;
    ERRNO = RHU_GERR_ENOTSUP;
    return -1;
}

int32_t SystemNative_GetPeerName(intptr_t socket, uint8_t* socketAddress, int32_t* socketAddressLen)
{
    (void)socket;
    (void)socketAddress;
    (void)socketAddressLen;
    return RHU_PAL_ENOTSUP;
}

int32_t SystemNative_GetPort(const uint8_t* socketAddress, int32_t socketAddressLen, uint16_t* port)
{
    (void)socketAddress;
    (void)socketAddressLen;
    (void)port;
    return RHU_PAL_ENOTSUP;
}

int32_t SystemNative_GetRawSockOpt(intptr_t socket, int32_t socketOptionLevel, int32_t socketOptionName, uint8_t* optionValue, int32_t* optionLen)
{
    (void)socket;
    (void)socketOptionLevel;
    (void)socketOptionName;
    (void)optionValue;
    (void)optionLen;
    return RHU_PAL_ENOTSUP;
}

int32_t SystemNative_GetSockName(intptr_t socket, uint8_t* socketAddress, int32_t* socketAddressLen)
{
    (void)socket;
    (void)socketAddress;
    (void)socketAddressLen;
    return RHU_PAL_ENOTSUP;
}

int32_t SystemNative_GetSockOpt(intptr_t socket, int32_t socketOptionLevel, int32_t socketOptionName, uint8_t* optionValue, int32_t* optionLen)
{
    (void)socket;
    (void)socketOptionLevel;
    (void)socketOptionName;
    (void)optionValue;
    (void)optionLen;
    return RHU_PAL_ENOTSUP;
}

int32_t SystemNative_GetSocketAddressSizes(int32_t* ipv4SocketAddressSize, int32_t* ipv6SocketAddressSize, int32_t* udsSocketAddressSize, int32_t* maxSocketAddressSize)
{
    (void)ipv4SocketAddressSize;
    (void)ipv6SocketAddressSize;
    (void)udsSocketAddressSize;
    (void)maxSocketAddressSize;
    return RHU_PAL_ENOTSUP;
}

int32_t SystemNative_GetSocketErrorOption(intptr_t socket, int32_t* error)
{
    (void)socket;
    (void)error;
    return RHU_PAL_ENOTSUP;
}

int32_t SystemNative_GetSocketType(intptr_t socket, int32_t* addressFamily, int32_t* socketType, int32_t* protocolType, int32_t* isListening)
{
    (void)socket;
    (void)addressFamily;
    (void)socketType;
    (void)protocolType;
    (void)isListening;
    return RHU_PAL_ENOTSUP;
}

int32_t SystemNative_GetTcpGlobalStatistics(TcpGlobalStatistics* retStats)
{
    (void)retStats;
    ERRNO = RHU_GERR_ENOTSUP;
    return -1;
}

int32_t SystemNative_GetUdpGlobalStatistics(UdpGlobalStatistics* retStats)
{
    (void)retStats;
    ERRNO = RHU_GERR_ENOTSUP;
    return -1;
}

int32_t SystemNative_GetWasiSocketDescriptor(intptr_t socket, void** entry, int32_t* socketType)
{
    (void)socket;
    (void)entry;
    (void)socketType;
    return RHU_PAL_ENOTSUP;
}

int32_t SystemNative_INotifyAddWatch(intptr_t fd, const char* pathName, uint32_t mask)
{
    (void)fd;
    (void)pathName;
    (void)mask;
    ERRNO = RHU_GERR_ENOTSUP;
    return -1;
}

intptr_t SystemNative_INotifyInit(void)
{
    ERRNO = RHU_GERR_ENOTSUP;
    return -1;
}

int32_t SystemNative_INotifyRemoveWatch(intptr_t fd, int32_t wd)
{
    (void)fd;
    (void)wd;
    ERRNO = RHU_GERR_ENOTSUP;
    return -1;
}

uint32_t SystemNative_InterfaceNameToIndex(char* interfaceName)
{
    (void)interfaceName;
    return 0;
}

int32_t SystemNative_Kill(int32_t pid, int32_t signal)
{
    (void)pid;
    (void)signal;
    ERRNO = RHU_GERR_EPERM;
    return -1;
}

int32_t SystemNative_LChflags(const char* path, uint32_t flags)
{
    (void)path;
    (void)flags;
    ERRNO = RHU_GERR_ENOTSUP;
    return -1;
}

int32_t SystemNative_Link(const char* source, const char* linkTarget)
{
    (void)source;
    (void)linkTarget;
    ERRNO = RHU_GERR_ENOTSUP;
    return -1;
}

int32_t SystemNative_Listen(intptr_t socket, int32_t backlog)
{
    (void)socket;
    (void)backlog;
    return RHU_PAL_ENOTSUP;
}

int32_t SystemNative_MapTcpState(int32_t tcpState)
{
    (void)tcpState;
    return 0;
}

int32_t SystemNative_MkFifo(const char* pathName, uint32_t mode)
{
    (void)pathName;
    (void)mode;
    ERRNO = RHU_GERR_ENOTSUP;
    return -1;
}

int32_t SystemNative_MkNod(const char* pathName, uint32_t mode, uint32_t major, uint32_t minor)
{
    (void)pathName;
    (void)mode;
    (void)major;
    (void)minor;
    ERRNO = RHU_GERR_ENOTSUP;
    return -1;
}

int32_t SystemNative_PlatformSupportsDualModeIPv4PacketInfo(void)
{
    return 0;
}

int32_t SystemNative_ReadEvents(intptr_t sock, NetworkChangeEvent onNetworkChange)
{
    (void)sock;
    (void)onNetworkChange;
    return RHU_PAL_ENOTSUP;
}

int32_t SystemNative_ReadLink(const char* path, char* buffer, int32_t bufferSize)
{
    (void)path;
    (void)buffer;
    (void)bufferSize;
    ERRNO = RHU_GERR_ENOTSUP;
    return -1;
}

int32_t SystemNative_ReadProcessInfo(int32_t pid, ProcessInfo* processInfo, uint8_t* argBuf, int32_t argBufSize)
{
    (void)pid;
    (void)processInfo;
    (void)argBuf;
    (void)argBufSize;
    ERRNO = RHU_GERR_ENOSYS;
    return -1;
}

int32_t SystemNative_ReadThreadInfo(int32_t pid, int32_t tid, ThreadInfo* threadInfo)
{
    (void)pid;
    (void)tid;
    (void)threadInfo;
    ERRNO = RHU_GERR_ENOSYS;
    return -1;
}

int32_t SystemNative_Receive(intptr_t socket, void* buffer, int32_t bufferLen, int32_t flags, int32_t* received)
{
    (void)socket;
    (void)buffer;
    (void)bufferLen;
    (void)flags;
    (void)received;
    return RHU_PAL_ENOTSUP;
}

int32_t SystemNative_ReceiveMessage(intptr_t socket, MessageHeader* messageHeader, int32_t flags, int64_t* received)
{
    (void)socket;
    (void)messageHeader;
    (void)flags;
    (void)received;
    return RHU_PAL_ENOTSUP;
}

int32_t SystemNative_ReceiveSocketError(intptr_t socket, MessageHeader* messageHeader)
{
    (void)socket;
    (void)messageHeader;
    return RHU_PAL_ENOTSUP;
}

int32_t SystemNative_Select(int* readFds, int readFdsCount, int* writeFds, int writeFdsCount, int* errorFds, int errorFdsCount, int32_t microseconds, int32_t maxFd, int* triggered)
{
    (void)readFds;
    (void)readFdsCount;
    (void)writeFds;
    (void)writeFdsCount;
    (void)errorFds;
    (void)errorFdsCount;
    (void)microseconds;
    (void)maxFd;
    (void)triggered;
    return RHU_PAL_ENOTSUP;
}

int32_t SystemNative_Send(intptr_t socket, void* buffer, int32_t bufferLen, int32_t flags, int32_t* sent)
{
    (void)socket;
    (void)buffer;
    (void)bufferLen;
    (void)flags;
    (void)sent;
    return RHU_PAL_ENOTSUP;
}

int32_t SystemNative_SendFile(intptr_t out_fd, intptr_t in_fd, int64_t offset, int64_t count, int64_t* sent)
{
    (void)out_fd;
    (void)in_fd;
    (void)offset;
    (void)count;
    (void)sent;
    return RHU_PAL_ENOTSUP;
}

int32_t SystemNative_SendMessage(intptr_t socket, MessageHeader* messageHeader, int32_t flags, int64_t* sent)
{
    (void)socket;
    (void)messageHeader;
    (void)flags;
    (void)sent;
    return RHU_PAL_ENOTSUP;
}

int32_t SystemNative_SetAddressFamily(uint8_t* socketAddress, int32_t socketAddressLen, int32_t addressFamily)
{
    (void)socketAddress;
    (void)socketAddressLen;
    (void)addressFamily;
    return RHU_PAL_ENOTSUP;
}

int32_t SystemNative_SetEUid(uint32_t euid)
{
    (void)euid;
    ERRNO = RHU_GERR_EPERM;
    return -1;
}

int32_t SystemNative_SetIPv4Address(uint8_t* socketAddress, int32_t socketAddressLen, uint32_t address)
{
    (void)socketAddress;
    (void)socketAddressLen;
    (void)address;
    return RHU_PAL_ENOTSUP;
}

int32_t SystemNative_SetIPv4MulticastOption(intptr_t socket, int32_t multicastOption, IPv4MulticastOption* option)
{
    (void)socket;
    (void)multicastOption;
    (void)option;
    return RHU_PAL_ENOTSUP;
}

int32_t SystemNative_SetIPv6Address(uint8_t* socketAddress, int32_t socketAddressLen, uint8_t* address, int32_t addressLen, uint32_t scopeId)
{
    (void)socketAddress;
    (void)socketAddressLen;
    (void)address;
    (void)addressLen;
    (void)scopeId;
    return RHU_PAL_ENOTSUP;
}

int32_t SystemNative_SetIPv6MulticastOption(intptr_t socket, int32_t multicastOption, IPv6MulticastOption* option)
{
    (void)socket;
    (void)multicastOption;
    (void)option;
    return RHU_PAL_ENOTSUP;
}

int32_t SystemNative_SetLingerOption(intptr_t socket, LingerOption* option)
{
    (void)socket;
    (void)option;
    return RHU_PAL_ENOTSUP;
}

int32_t SystemNative_SetPort(uint8_t* socketAddress, int32_t socketAddressLen, uint16_t port)
{
    (void)socketAddress;
    (void)socketAddressLen;
    (void)port;
    return RHU_PAL_ENOTSUP;
}

int32_t SystemNative_SetRawSockOpt(intptr_t socket, int32_t socketOptionLevel, int32_t socketOptionName, uint8_t* optionValue, int32_t optionLen)
{
    (void)socket;
    (void)socketOptionLevel;
    (void)socketOptionName;
    (void)optionValue;
    (void)optionLen;
    return RHU_PAL_ENOTSUP;
}

int32_t SystemNative_SetReceiveTimeout(intptr_t socket, int32_t millisecondsTimeout)
{
    (void)socket;
    (void)millisecondsTimeout;
    return RHU_PAL_ENOTSUP;
}

int32_t SystemNative_SetSendTimeout(intptr_t socket, int32_t millisecondsTimeout)
{
    (void)socket;
    (void)millisecondsTimeout;
    return RHU_PAL_ENOTSUP;
}

int32_t SystemNative_SetSockOpt(intptr_t socket, int32_t socketOptionLevel, int32_t socketOptionName, uint8_t* optionValue, int32_t optionLen)
{
    (void)socket;
    (void)socketOptionLevel;
    (void)socketOptionName;
    (void)optionValue;
    (void)optionLen;
    return RHU_PAL_ENOTSUP;
}

int32_t SystemNative_Shutdown(intptr_t socket, int32_t socketShutdown)
{
    (void)socket;
    (void)socketShutdown;
    return RHU_PAL_ENOTSUP;
}

int32_t SystemNative_Socket(int32_t addressFamily, int32_t socketType, int32_t protocolType, intptr_t* createdSocket)
{
    (void)addressFamily;
    (void)socketType;
    (void)protocolType;
    (void)createdSocket;
    return RHU_PAL_EAFNOSUPPORT;
}

int32_t SystemNative_SymLink(const char* target, const char* linkPath)
{
    (void)target;
    (void)linkPath;
    ERRNO = RHU_GERR_ENOTSUP;
    return -1;
}

int32_t SystemNative_Sysctl(int* name, unsigned int namelen, void* value, size_t* len)
{
    (void)name;
    (void)namelen;
    (void)value;
    (void)len;
    ERRNO = RHU_GERR_ENOTSUP;
    return -1;
}

int32_t SystemNative_TryChangeSocketEventRegistration(intptr_t port, intptr_t socket, int32_t currentEvents, int32_t newEvents, uintptr_t data)
{
    (void)port;
    (void)socket;
    (void)currentEvents;
    (void)newEvents;
    (void)data;
    return RHU_PAL_ENOTSUP;
}

int32_t SystemNative_TryGetIPPacketInformation(MessageHeader* messageHeader, int32_t isIPv4, IPPacketInformation* packetInfo)
{
    (void)messageHeader;
    (void)isIPv4;
    (void)packetInfo;
    return RHU_PAL_ENOTSUP;
}

int32_t SystemNative_WaitForSocketEvents(intptr_t port, SocketEvent* buffer, int32_t* count)
{
    (void)port;
    (void)buffer;
    (void)count;
    return RHU_PAL_ENOTSUP;
}

int32_t SystemNative_WaitIdAnyExitedNoHangNoWait(void)
{
    ERRNO = RHU_GERR_ECHILD;
    return -1;
}

int32_t SystemNative_WaitPidExitedNoHang(int32_t pid, int32_t* exitCode, int32_t* terminatingSignal)
{
    (void)pid;
    (void)exitCode;
    (void)terminatingSignal;
    ERRNO = RHU_GERR_ECHILD;
    return -1;
}

int32_t SystemNative_WasiSubscribeSocketPollable(int32_t kind, int32_t handle)
{
    (void)kind;
    (void)handle;
    return RHU_PAL_ENOTSUP;
}

/* ------------------------------------------------------------------ */
/* GlobalizationNative_ */
/* ------------------------------------------------------------------ */

void GlobalizationNative_ChangeCase(const uint16_t* lpSrc, int32_t cwSrcLength, uint16_t* lpDst, int32_t cwDstLength, int32_t bToUpper)
{
    (void)lpSrc;
    (void)cwSrcLength;
    (void)lpDst;
    (void)cwDstLength;
    (void)bToUpper;
}

void GlobalizationNative_ChangeCaseInvariant(const uint16_t* lpSrc, int32_t cwSrcLength, uint16_t* lpDst, int32_t cwDstLength, int32_t bToUpper)
{
    (void)lpSrc;
    (void)cwSrcLength;
    (void)lpDst;
    (void)cwDstLength;
    (void)bToUpper;
}

void GlobalizationNative_ChangeCaseTurkish(const uint16_t* lpSrc, int32_t cwSrcLength, uint16_t* lpDst, int32_t cwDstLength, int32_t bToUpper)
{
    (void)lpSrc;
    (void)cwSrcLength;
    (void)lpDst;
    (void)cwDstLength;
    (void)bToUpper;
}

void GlobalizationNative_CloseSortHandle(SortHandle* pSortHandle)
{
    (void)pSortHandle;
}

int32_t GlobalizationNative_CompareString(SortHandle* pSortHandle, const uint16_t* lpStr1, int32_t cwStr1Length, const uint16_t* lpStr2, int32_t cwStr2Length, int32_t options)
{
    (void)pSortHandle;
    (void)lpStr1;
    (void)cwStr1Length;
    (void)lpStr2;
    (void)cwStr2Length;
    (void)options;
    return 0;
}

int32_t GlobalizationNative_EndsWith(SortHandle* pSortHandle, const uint16_t* lpTarget, int32_t cwTargetLength, const uint16_t* lpSource, int32_t cwSourceLength, int32_t options, int32_t* pMatchedLength)
{
    (void)pSortHandle;
    (void)lpTarget;
    (void)cwTargetLength;
    (void)lpSource;
    (void)cwSourceLength;
    (void)options;
    (void)pMatchedLength;
    return 0;
}

int32_t GlobalizationNative_EnumCalendarInfo(EnumCalendarInfoCallback callback, const uint16_t* localeName, uint16_t calendarId, int32_t dataType, const void* context)
{
    (void)callback;
    (void)localeName;
    (void)calendarId;
    (void)dataType;
    (void)context;
    return 0;
}

int32_t GlobalizationNative_GetCalendarInfo(const uint16_t* localeName, uint16_t calendarId, int32_t dataType, uint16_t* result, int32_t resultCapacity)
{
    (void)localeName;
    (void)calendarId;
    (void)dataType;
    (void)result;
    (void)resultCapacity;
    return RHU_PAL_UnknownError;
}

int32_t GlobalizationNative_GetCalendars(const uint16_t* localeName, uint16_t* calendars, int32_t calendarsCapacity)
{
    (void)localeName;
    (void)calendars;
    (void)calendarsCapacity;
    return 0;
}

int32_t GlobalizationNative_GetDefaultLocaleName(uint16_t* value, int32_t valueLength)
{
    (void)value;
    (void)valueLength;
    return 0;
}

int32_t GlobalizationNative_GetICUVersion(void)
{
    return 0;
}

int32_t GlobalizationNative_GetJapaneseEraStartDate(int32_t era, int32_t* startYear, int32_t* startMonth, int32_t* startDay)
{
    (void)era;
    (void)startYear;
    (void)startMonth;
    (void)startDay;
    return 0;
}

int32_t GlobalizationNative_GetLatestJapaneseEra(void)
{
    return 0;
}

int32_t GlobalizationNative_GetLocaleInfoGroupingSizes(const uint16_t* localeName, int32_t localeGroupingData, int32_t* primaryGroupSize, int32_t* secondaryGroupSize)
{
    (void)localeName;
    (void)localeGroupingData;
    (void)primaryGroupSize;
    (void)secondaryGroupSize;
    return 0;
}

int32_t GlobalizationNative_GetLocaleInfoInt(const uint16_t* localeName, int32_t localeNumberData, int32_t* value)
{
    (void)localeName;
    (void)localeNumberData;
    (void)value;
    return 0;
}

int32_t GlobalizationNative_GetLocaleInfoString(const uint16_t* localeName, int32_t localeStringData, uint16_t* value, int32_t valueLength, const uint16_t* uiLocaleName)
{
    (void)localeName;
    (void)localeStringData;
    (void)value;
    (void)valueLength;
    (void)uiLocaleName;
    return 0;
}

int32_t GlobalizationNative_GetLocaleName(const uint16_t* localeName, uint16_t* value, int32_t valueLength)
{
    (void)localeName;
    (void)value;
    (void)valueLength;
    return 0;
}

int32_t GlobalizationNative_GetLocaleTimeFormat(const uint16_t* localeName, int shortFormat, uint16_t* value, int32_t valueLength)
{
    (void)localeName;
    (void)shortFormat;
    (void)value;
    (void)valueLength;
    return 0;
}

int32_t GlobalizationNative_GetLocales(uint16_t* value, int32_t valueLength)
{
    (void)value;
    (void)valueLength;
    return 0;
}

int32_t GlobalizationNative_GetSortHandle(const char* lpLocaleName, SortHandle** ppSortHandle)
{
    (void)lpLocaleName;
    (void)ppSortHandle;
    return RHU_PAL_UnknownError;
}

int32_t GlobalizationNative_GetSortKey(SortHandle* pSortHandle, const uint16_t* lpStr, int32_t cwStrLength, uint8_t* sortKey, int32_t cbSortKeyLength, int32_t options)
{
    (void)pSortHandle;
    (void)lpStr;
    (void)cwStrLength;
    (void)sortKey;
    (void)cbSortKeyLength;
    (void)options;
    return 0;
}

int32_t GlobalizationNative_GetSortVersion(SortHandle* pSortHandle)
{
    (void)pSortHandle;
    return 0;
}

int32_t GlobalizationNative_GetTimeZoneDisplayName(const uint16_t* localeName, const uint16_t* timeZoneId, int32_t type, uint16_t* result, int32_t resultLength)
{
    (void)localeName;
    (void)timeZoneId;
    (void)type;
    (void)result;
    (void)resultLength;
    return RHU_PAL_UnknownError;
}

int32_t GlobalizationNative_IanaIdToWindowsId(const uint16_t* ianaId, uint16_t* windowsId, int32_t windowsIdLength)
{
    (void)ianaId;
    (void)windowsId;
    (void)windowsIdLength;
    return 0;
}

int32_t GlobalizationNative_IndexOf(SortHandle* pSortHandle, const uint16_t* lpTarget, int32_t cwTargetLength, const uint16_t* lpSource, int32_t cwSourceLength, int32_t options, int32_t* pMatchedLength)
{
    (void)pSortHandle;
    (void)lpTarget;
    (void)cwTargetLength;
    (void)lpSource;
    (void)cwSourceLength;
    (void)options;
    (void)pMatchedLength;
    return 0;
}

void GlobalizationNative_InitICUFunctions(void* icuuc, void* icuin, const char* version, const char* suffix)
{
    (void)icuuc;
    (void)icuin;
    (void)version;
    (void)suffix;
}

void GlobalizationNative_InitOrdinalCasingPage(int32_t pageNumber, uint16_t* pTarget)
{
    (void)pageNumber;
    (void)pTarget;
}

void GlobalizationNative_InitOrdinalLowerCasingPage(int32_t pageNumber, uint16_t* pTarget)
{
    (void)pageNumber;
    (void)pTarget;
}

int32_t GlobalizationNative_IsNormalized(int32_t normalizationForm, const uint16_t* lpStr, int32_t cwStrLength)
{
    (void)normalizationForm;
    (void)lpStr;
    (void)cwStrLength;
    return 0;
}

int32_t GlobalizationNative_IsPredefinedLocale(const uint16_t* localeName)
{
    (void)localeName;
    return 0;
}

int32_t GlobalizationNative_LastIndexOf(SortHandle* pSortHandle, const uint16_t* lpTarget, int32_t cwTargetLength, const uint16_t* lpSource, int32_t cwSourceLength, int32_t options, int32_t* pMatchedLength)
{
    (void)pSortHandle;
    (void)lpTarget;
    (void)cwTargetLength;
    (void)lpSource;
    (void)cwSourceLength;
    (void)options;
    (void)pMatchedLength;
    return 0;
}

int32_t GlobalizationNative_LoadICU(void)
{
    return 0;
}

int32_t GlobalizationNative_NormalizeString(int32_t normalizationForm, const uint16_t* lpSrc, int32_t cwSrcLength, uint16_t* lpDst, int32_t cwDstLength)
{
    (void)normalizationForm;
    (void)lpSrc;
    (void)cwSrcLength;
    (void)lpDst;
    (void)cwDstLength;
    return 0;
}

int32_t GlobalizationNative_StartsWith(SortHandle* pSortHandle, const uint16_t* lpTarget, int32_t cwTargetLength, const uint16_t* lpSource, int32_t cwSourceLength, int32_t options, int32_t* pMatchedLength)
{
    (void)pSortHandle;
    (void)lpTarget;
    (void)cwTargetLength;
    (void)lpSource;
    (void)cwSourceLength;
    (void)options;
    (void)pMatchedLength;
    return 0;
}

int32_t GlobalizationNative_ToAscii(uint32_t flags, const uint16_t* lpSrc, int32_t cwSrcLength, uint16_t* lpDst, int32_t cwDstLength)
{
    (void)flags;
    (void)lpSrc;
    (void)cwSrcLength;
    (void)lpDst;
    (void)cwDstLength;
    return 0;
}

int32_t GlobalizationNative_ToUnicode(uint32_t flags, const uint16_t* lpSrc, int32_t cwSrcLength, uint16_t* lpDst, int32_t cwDstLength)
{
    (void)flags;
    (void)lpSrc;
    (void)cwSrcLength;
    (void)lpDst;
    (void)cwDstLength;
    return 0;
}

int32_t GlobalizationNative_WindowsIdToIanaId(const uint16_t* windowsId, const char* region, uint16_t* ianaId, int32_t ianaIdLength)
{
    (void)windowsId;
    (void)region;
    (void)ianaId;
    (void)ianaIdLength;
    return 0;
}

/* ------------------------------------------------------------------ */
/* CompressionNative_ */
/* ------------------------------------------------------------------ */

uint32_t CompressionNative_CompressBound(uint32_t sourceLen)
{
    (void)sourceLen;
    return 0;
}

uint32_t CompressionNative_Crc32(uint32_t crc, uint8_t* buffer, int32_t len)
{
    (void)crc;
    (void)buffer;
    (void)len;
    return 0;
}

int32_t CompressionNative_Deflate(PAL_ZStream* stream, int32_t flush)
{
    (void)stream;
    (void)flush;
    return -6;
}

int32_t CompressionNative_DeflateEnd(PAL_ZStream* stream)
{
    (void)stream;
    return -6;
}

int32_t CompressionNative_DeflateInit2_(PAL_ZStream* stream, int32_t level, int32_t method, int32_t windowBits, int32_t memLevel, int32_t strategy)
{
    (void)stream;
    (void)level;
    (void)method;
    (void)windowBits;
    (void)memLevel;
    (void)strategy;
    return -6;
}

int32_t CompressionNative_DeflateReset(PAL_ZStream* stream)
{
    (void)stream;
    return -6;
}

int32_t CompressionNative_Inflate(PAL_ZStream* stream, int32_t flush)
{
    (void)stream;
    (void)flush;
    return -6;
}

int32_t CompressionNative_InflateEnd(PAL_ZStream* stream)
{
    (void)stream;
    return -6;
}

int32_t CompressionNative_InflateInit2_(PAL_ZStream* stream, int32_t windowBits)
{
    (void)stream;
    (void)windowBits;
    return -6;
}

int32_t CompressionNative_InflateReset2_(PAL_ZStream* stream, int32_t windowBits)
{
    (void)stream;
    (void)windowBits;
    return -6;
}

/* ------------------------------------------------------------------ */
/* NetSecurityNative_ */
/* ------------------------------------------------------------------ */

uint32_t NetSecurityNative_AcceptSecContext(uint32_t* minorStatus, GssCredId* acceptorCredHandle, GssCtxId** contextHandle, void* cbt, int32_t cbtSize, uint8_t* inputBytes, uint32_t inputLength, PAL_GssBuffer* outBuffer, uint32_t* retFlags, int32_t* isNtlmUsed)
{
    (void)minorStatus;
    (void)acceptorCredHandle;
    (void)contextHandle;
    (void)cbt;
    (void)cbtSize;
    (void)inputBytes;
    (void)inputLength;
    (void)outBuffer;
    (void)retFlags;
    (void)isNtlmUsed;
    return 0x00100000u;
}

uint32_t NetSecurityNative_AcquireAcceptorCred(uint32_t* minorStatus, GssCredId** outputCredHandle)
{
    (void)minorStatus;
    (void)outputCredHandle;
    return 0x00100000u;
}

uint32_t NetSecurityNative_DeleteSecContext(uint32_t* minorStatus, GssCtxId** contextHandle)
{
    (void)minorStatus;
    (void)contextHandle;
    return 0;
}

uint32_t NetSecurityNative_DisplayMajorStatus(uint32_t* minorStatus, uint32_t statusValue, PAL_GssBuffer* outBuffer)
{
    (void)minorStatus;
    (void)statusValue;
    (void)outBuffer;
    return 0x00100000u;
}

uint32_t NetSecurityNative_DisplayMinorStatus(uint32_t* minorStatus, uint32_t statusValue, PAL_GssBuffer* outBuffer)
{
    (void)minorStatus;
    (void)statusValue;
    (void)outBuffer;
    return 0x00100000u;
}

int32_t NetSecurityNative_EnsureGssInitialized(void)
{
    return -1;
}

uint32_t NetSecurityNative_GetMic(uint32_t* minorStatus, GssCtxId* contextHandle, uint8_t* inputBytes, int32_t inputLength, PAL_GssBuffer* outBuffer)
{
    (void)minorStatus;
    (void)contextHandle;
    (void)inputBytes;
    (void)inputLength;
    (void)outBuffer;
    return 0x00100000u;
}

uint32_t NetSecurityNative_GetUser(uint32_t* minorStatus, GssCtxId* contextHandle, PAL_GssBuffer* outBuffer)
{
    (void)minorStatus;
    (void)contextHandle;
    (void)outBuffer;
    return 0x00100000u;
}

uint32_t NetSecurityNative_ImportPrincipalName(uint32_t* minorStatus, char* inputName, uint32_t inputNameLen, GssName** outputName)
{
    (void)minorStatus;
    (void)inputName;
    (void)inputNameLen;
    (void)outputName;
    return 0x00100000u;
}

uint32_t NetSecurityNative_ImportUserName(uint32_t* minorStatus, char* inputName, uint32_t inputNameLen, GssName** outputName)
{
    (void)minorStatus;
    (void)inputName;
    (void)inputNameLen;
    (void)outputName;
    return 0x00100000u;
}

uint32_t NetSecurityNative_InitSecContext(uint32_t* minorStatus, GssCredId* claimantCredHandle, GssCtxId** contextHandle, uint32_t packageType, GssName* targetName, uint32_t reqFlags, uint8_t* inputBytes, uint32_t inputLength, PAL_GssBuffer* outBuffer, uint32_t* retFlags, int32_t* isNtlmUsed)
{
    (void)minorStatus;
    (void)claimantCredHandle;
    (void)contextHandle;
    (void)packageType;
    (void)targetName;
    (void)reqFlags;
    (void)inputBytes;
    (void)inputLength;
    (void)outBuffer;
    (void)retFlags;
    (void)isNtlmUsed;
    return 0x00100000u;
}

uint32_t NetSecurityNative_InitSecContextEx(uint32_t* minorStatus, GssCredId* claimantCredHandle, GssCtxId** contextHandle, uint32_t packageType, void* cbt, int32_t cbtSize, GssName* targetName, uint32_t reqFlags, uint8_t* inputBytes, uint32_t inputLength, PAL_GssBuffer* outBuffer, uint32_t* retFlags, int32_t* isNtlmUsed)
{
    (void)minorStatus;
    (void)claimantCredHandle;
    (void)contextHandle;
    (void)packageType;
    (void)cbt;
    (void)cbtSize;
    (void)targetName;
    (void)reqFlags;
    (void)inputBytes;
    (void)inputLength;
    (void)outBuffer;
    (void)retFlags;
    (void)isNtlmUsed;
    return 0x00100000u;
}

uint32_t NetSecurityNative_InitiateCredSpNego(uint32_t* minorStatus, GssName* desiredName, GssCredId** outputCredHandle)
{
    (void)minorStatus;
    (void)desiredName;
    (void)outputCredHandle;
    return 0x00100000u;
}

uint32_t NetSecurityNative_InitiateCredWithPassword(uint32_t* minorStatus, int32_t packageType, GssName* desiredName, char* password, uint32_t passwdLen, GssCredId** outputCredHandle)
{
    (void)minorStatus;
    (void)packageType;
    (void)desiredName;
    (void)password;
    (void)passwdLen;
    (void)outputCredHandle;
    return 0x00100000u;
}

uint32_t NetSecurityNative_IsNtlmInstalled(void)
{
    return 0;
}

uint32_t NetSecurityNative_ReleaseCred(uint32_t* minorStatus, GssCredId** credHandle)
{
    (void)minorStatus;
    (void)credHandle;
    return 0;
}

void NetSecurityNative_ReleaseGssBuffer(void* buffer, uint64_t length)
{
    (void)buffer;
    (void)length;
}

uint32_t NetSecurityNative_ReleaseName(uint32_t* minorStatus, GssName** inputName)
{
    (void)minorStatus;
    (void)inputName;
    return 0;
}

uint32_t NetSecurityNative_Unwrap(uint32_t* minorStatus, GssCtxId* contextHandle, int32_t* isEncrypt, uint8_t* inputBytes, int32_t count, PAL_GssBuffer* outBuffer)
{
    (void)minorStatus;
    (void)contextHandle;
    (void)isEncrypt;
    (void)inputBytes;
    (void)count;
    (void)outBuffer;
    return 0x00100000u;
}

uint32_t NetSecurityNative_VerifyMic(uint32_t* minorStatus, GssCtxId* contextHandle, uint8_t* inputBytes, int32_t inputLength, uint8_t* tokenBytes, int32_t tokenLength)
{
    (void)minorStatus;
    (void)contextHandle;
    (void)inputBytes;
    (void)inputLength;
    (void)tokenBytes;
    (void)tokenLength;
    return 0x00100000u;
}

uint32_t NetSecurityNative_Wrap(uint32_t* minorStatus, GssCtxId* contextHandle, int32_t* isEncrypt, uint8_t* inputBytes, int32_t count, PAL_GssBuffer* outBuffer)
{
    (void)minorStatus;
    (void)contextHandle;
    (void)isEncrypt;
    (void)inputBytes;
    (void)count;
    (void)outBuffer;
    return 0x00100000u;
}

/* ------------------------------------------------------------------ */
/* CryptoNative_ */
/* ------------------------------------------------------------------ */

void CryptoNative_Asn1ObjectFree(ASN1_OBJECT* a)
{
    (void)a;
}

void CryptoNative_Asn1OctetStringFree(ASN1_STRING* a)
{
    (void)a;
}

ASN1_OCTET_STRING* CryptoNative_Asn1OctetStringNew(void)
{
    return 0;
}

int32_t CryptoNative_Asn1OctetStringSet(ASN1_OCTET_STRING* s, const uint8_t* data, int32_t len)
{
    (void)s;
    (void)data;
    (void)len;
    return 0;
}

void CryptoNative_BigNumDestroy(BIGNUM* a)
{
    (void)a;
}

int32_t CryptoNative_BigNumToBinary(const BIGNUM* a, uint8_t* to)
{
    (void)a;
    (void)to;
    return 0;
}

void CryptoNative_BioClearReadWindow(BIO* bio, int32_t* leftoverLength)
{
    (void)bio;
    (void)leftoverLength;
}

int32_t CryptoNative_BioDestroy(BIO* a)
{
    (void)a;
    return 0;
}

int32_t CryptoNative_BioDrainSpill(BIO* bio, void* dst, int32_t dstLen)
{
    (void)bio;
    (void)dst;
    (void)dstLen;
    return 0;
}

int32_t CryptoNative_BioGetReplayPrefix(BIO* bio, uint8_t** outPtr, int32_t* outLen)
{
    (void)bio;
    (void)outPtr;
    (void)outLen;
    return 0;
}

void CryptoNative_BioGetWriteResult(BIO* bio, int32_t* writtenToWindow, int32_t* spillLen)
{
    (void)bio;
    (void)writtenToWindow;
    (void)spillLen;
}

int32_t CryptoNative_BioGets(BIO* b, char* buf, int32_t size)
{
    (void)b;
    (void)buf;
    (void)size;
    return 0;
}

BIO* CryptoNative_BioNewFile(const char* filename, const char* mode)
{
    (void)filename;
    (void)mode;
    return 0;
}

BIO* CryptoNative_BioNewManagedSpan(void)
{
    return 0;
}

BIO* CryptoNative_BioNewSocketReplay(intptr_t fd, const void* prefix, int32_t prefixLen)
{
    (void)fd;
    (void)prefix;
    (void)prefixLen;
    return 0;
}

int32_t CryptoNative_BioPeekTlsFrame(BIO* bio, uint8_t** outPtr, int32_t* outLen)
{
    (void)bio;
    (void)outPtr;
    (void)outLen;
    return 0;
}

int32_t CryptoNative_BioRead(BIO* b, void* buf, int32_t len)
{
    (void)b;
    (void)buf;
    (void)len;
    return 0;
}

int32_t CryptoNative_BioSeek(BIO* bio, int32_t ofs)
{
    (void)bio;
    (void)ofs;
    return 0;
}

void CryptoNative_BioSetReadWindow(BIO* bio, const void* ptr, int32_t len)
{
    (void)bio;
    (void)ptr;
    (void)len;
}

void CryptoNative_BioSetWriteWindow(BIO* bio, void* ptr, int32_t capacity)
{
    (void)bio;
    (void)ptr;
    (void)capacity;
}

int32_t CryptoNative_BioTell(BIO* bio)
{
    (void)bio;
    return 0;
}

int32_t CryptoNative_BioWrite(BIO* b, const void* buf, int32_t len)
{
    (void)b;
    (void)buf;
    (void)len;
    return 0;
}

int32_t CryptoNative_CheckX509Hostname(X509* x509, const char* hostname, int32_t cchHostname)
{
    (void)x509;
    (void)hostname;
    (void)cchHostname;
    return 0;
}

int32_t CryptoNative_CheckX509IpAddress(X509* x509, const uint8_t* addressBytes, int32_t addressBytesLen, const char* hostname, int32_t cchHostname)
{
    (void)x509;
    (void)addressBytes;
    (void)addressBytesLen;
    (void)hostname;
    (void)cchHostname;
    return 0;
}

EVP_PKEY* CryptoNative_CreateEvpPkeyFromEcKey(EC_KEY* ecKey, int32_t* outKeySize)
{
    (void)ecKey;
    (void)outKeySize;
    return 0;
}

BIO* CryptoNative_CreateMemoryBio(void)
{
    return 0;
}

PKCS7* CryptoNative_D2IPkcs7Bio(BIO* bp)
{
    (void)bp;
    return 0;
}

OCSP_RESPONSE* CryptoNative_DecodeOcspResponse(const uint8_t* buf, int32_t len)
{
    (void)buf;
    (void)len;
    return 0;
}

PKCS7* CryptoNative_DecodePkcs7(const uint8_t* buf, int32_t len)
{
    (void)buf;
    (void)len;
    return 0;
}

EVP_PKEY* CryptoNative_DecodePkcs8PrivateKey(const uint8_t* buf, int32_t len, int32_t algId)
{
    (void)buf;
    (void)len;
    (void)algId;
    return 0;
}

EVP_PKEY* CryptoNative_DecodeSubjectPublicKeyInfo(const uint8_t* buf, int32_t len, int32_t algId)
{
    (void)buf;
    (void)len;
    (void)algId;
    return 0;
}

X509* CryptoNative_DecodeX509(const uint8_t* buf, int32_t len)
{
    (void)buf;
    (void)len;
    return 0;
}

X509_CRL* CryptoNative_DecodeX509Crl(const uint8_t* buf, int32_t len)
{
    (void)buf;
    (void)len;
    return 0;
}

void CryptoNative_DsaDestroy(DSA* dsa)
{
    (void)dsa;
}

int32_t CryptoNative_DsaGenerateKey(DSA** dsa, int32_t bits)
{
    (void)dsa;
    (void)bits;
    return 0;
}

int32_t CryptoNative_DsaKeyCreateByExplicitParameters(DSA** dsa, uint8_t* p, int32_t pLength, uint8_t* q, int32_t qLength, uint8_t* g, int32_t gLength, uint8_t* y, int32_t yLength, uint8_t* x, int32_t xLength)
{
    (void)dsa;
    (void)p;
    (void)pLength;
    (void)q;
    (void)qLength;
    (void)g;
    (void)gLength;
    (void)y;
    (void)yLength;
    (void)x;
    (void)xLength;
    return 0;
}

int32_t CryptoNative_DsaSign(DSA* dsa, const uint8_t* hash, int32_t hashLength, uint8_t* signature, int32_t* outSignatureLength)
{
    (void)dsa;
    (void)hash;
    (void)hashLength;
    (void)signature;
    (void)outSignatureLength;
    return 0;
}

int32_t CryptoNative_DsaSizeP(DSA* dsa)
{
    (void)dsa;
    return 0;
}

int32_t CryptoNative_DsaSizeQ(DSA* dsa)
{
    (void)dsa;
    return 0;
}

int32_t CryptoNative_DsaSizeSignature(DSA* dsa)
{
    (void)dsa;
    return 0;
}

int32_t CryptoNative_DsaUpRef(DSA* dsa)
{
    (void)dsa;
    return 0;
}

int32_t CryptoNative_DsaVerify(DSA* dsa, const uint8_t* hash, int32_t hashLength, uint8_t* signature, int32_t signatureLength)
{
    (void)dsa;
    (void)hash;
    (void)hashLength;
    (void)signature;
    (void)signatureLength;
    return 0;
}

int32_t CryptoNative_EcDsaSignHash(EVP_PKEY* pkey, void* extraHandle, const uint8_t* hash, int32_t hashLen, uint8_t* destination, int32_t destinationLen)
{
    (void)pkey;
    (void)extraHandle;
    (void)hash;
    (void)hashLen;
    (void)destination;
    (void)destinationLen;
    return 0;
}

int32_t CryptoNative_EcDsaVerifyHash(EVP_PKEY* pkey, void* extraHandle, const uint8_t* hash, int32_t hashLen, const uint8_t* signature, int32_t signatureLen)
{
    (void)pkey;
    (void)extraHandle;
    (void)hash;
    (void)hashLen;
    (void)signature;
    (void)signatureLen;
    return 0;
}

EC_KEY* CryptoNative_EcKeyCreateByExplicitParameters(int32_t curveType, const uint8_t* qx, int32_t qxLength, const uint8_t* qy, int32_t qyLength, const uint8_t* d, int32_t dLength, const uint8_t* p, int32_t pLength, const uint8_t* a, int32_t aLength, const uint8_t* b, int32_t bLength, const uint8_t* gx, int32_t gxLength, const uint8_t* gy, int32_t gyLength, const uint8_t* order, int32_t nLength, const uint8_t* cofactor, int32_t hLength, const uint8_t* seed, int32_t sLength)
{
    (void)curveType;
    (void)qx;
    (void)qxLength;
    (void)qy;
    (void)qyLength;
    (void)d;
    (void)dLength;
    (void)p;
    (void)pLength;
    (void)a;
    (void)aLength;
    (void)b;
    (void)bLength;
    (void)gx;
    (void)gxLength;
    (void)gy;
    (void)gyLength;
    (void)order;
    (void)nLength;
    (void)cofactor;
    (void)hLength;
    (void)seed;
    (void)sLength;
    return 0;
}

int32_t CryptoNative_EcKeyCreateByKeyParameters(EC_KEY** key, const char* oid, const uint8_t* qx, int32_t qxLength, const uint8_t* qy, int32_t qyLength, const uint8_t* d, int32_t dLength)
{
    (void)key;
    (void)oid;
    (void)qx;
    (void)qxLength;
    (void)qy;
    (void)qyLength;
    (void)d;
    (void)dLength;
    return 0;
}

EC_KEY* CryptoNative_EcKeyCreateByOid(const char* oid)
{
    (void)oid;
    return 0;
}

void CryptoNative_EcKeyDestroy(EC_KEY* r)
{
    (void)r;
}

int32_t CryptoNative_EcKeyGenerateKey(EC_KEY* eckey)
{
    (void)eckey;
    return 0;
}

int32_t CryptoNative_EcKeyGetCurveName2(const EC_KEY* key, int32_t* nidName)
{
    (void)key;
    (void)nidName;
    return 0;
}

int32_t CryptoNative_EcKeyGetSize(const EC_KEY* key, int32_t* keySize)
{
    (void)key;
    (void)keySize;
    return 0;
}

int32_t CryptoNative_EcKeyUpRef(EC_KEY* r)
{
    (void)r;
    return 0;
}

void CryptoNative_EnableMemoryTracking(int32_t enable)
{
    (void)enable;
}

int32_t CryptoNative_EncodeAsn1Integer(ASN1_INTEGER* i, uint8_t* buf)
{
    (void)i;
    (void)buf;
    return 0;
}

int32_t CryptoNative_EncodeOcspRequest(OCSP_REQUEST* req, uint8_t* buf)
{
    (void)req;
    (void)buf;
    return 0;
}

int32_t CryptoNative_EncodePkcs7(PKCS7* p7, uint8_t* buf)
{
    (void)p7;
    (void)buf;
    return 0;
}

int32_t CryptoNative_EncodePkcs8PrivateKey(EVP_PKEY* pkey, uint8_t* buf)
{
    (void)pkey;
    (void)buf;
    return 0;
}

int32_t CryptoNative_EncodeSubjectPublicKeyInfo(EVP_PKEY* pkey, uint8_t* buf)
{
    (void)pkey;
    (void)buf;
    return 0;
}

int32_t CryptoNative_EncodeX509(X509* x, uint8_t* buf)
{
    (void)x;
    (void)buf;
    return 0;
}

int32_t CryptoNative_EncodeX509SubjectPublicKeyInfo(X509* x, uint8_t* buf)
{
    (void)x;
    (void)buf;
    return 0;
}

void CryptoNative_EnsureLibSslInitialized(void)
{
}

int32_t CryptoNative_EnsureOpenSslInitialized(void)
{
    return 1;
}

void CryptoNative_ErrClearError(void)
{
}

void CryptoNative_ErrErrorStringN(uint64_t e, char* buf, int32_t len)
{
    (void)e;
    (void)buf;
    (void)len;
}

uint64_t CryptoNative_ErrGetExceptionError(int32_t* isAllocFailure)
{
    (void)isAllocFailure;
    return 0;
}

uint64_t CryptoNative_ErrPeekError(void)
{
    return 0;
}

uint64_t CryptoNative_ErrPeekLastError(void)
{
    return 0;
}

const char* CryptoNative_ErrReasonErrorString(uint64_t error)
{
    (void)error;
    return 0;
}

const EVP_CIPHER* CryptoNative_EvpAes128Cbc(void)
{
    return 0;
}

const EVP_CIPHER* CryptoNative_EvpAes128Ccm(void)
{
    return 0;
}

const EVP_CIPHER* CryptoNative_EvpAes128Cfb128(void)
{
    return 0;
}

const EVP_CIPHER* CryptoNative_EvpAes128Cfb8(void)
{
    return 0;
}

const EVP_CIPHER* CryptoNative_EvpAes128Ecb(void)
{
    return 0;
}

const EVP_CIPHER* CryptoNative_EvpAes128Gcm(void)
{
    return 0;
}

const EVP_CIPHER* CryptoNative_EvpAes128Wrap(void)
{
    return 0;
}

const EVP_CIPHER* CryptoNative_EvpAes128WrapPad(void)
{
    return 0;
}

const EVP_CIPHER* CryptoNative_EvpAes192Cbc(void)
{
    return 0;
}

const EVP_CIPHER* CryptoNative_EvpAes192Ccm(void)
{
    return 0;
}

const EVP_CIPHER* CryptoNative_EvpAes192Cfb128(void)
{
    return 0;
}

const EVP_CIPHER* CryptoNative_EvpAes192Cfb8(void)
{
    return 0;
}

const EVP_CIPHER* CryptoNative_EvpAes192Ecb(void)
{
    return 0;
}

const EVP_CIPHER* CryptoNative_EvpAes192Gcm(void)
{
    return 0;
}

const EVP_CIPHER* CryptoNative_EvpAes192Wrap(void)
{
    return 0;
}

const EVP_CIPHER* CryptoNative_EvpAes192WrapPad(void)
{
    return 0;
}

const EVP_CIPHER* CryptoNative_EvpAes256Cbc(void)
{
    return 0;
}

const EVP_CIPHER* CryptoNative_EvpAes256Ccm(void)
{
    return 0;
}

const EVP_CIPHER* CryptoNative_EvpAes256Cfb128(void)
{
    return 0;
}

const EVP_CIPHER* CryptoNative_EvpAes256Cfb8(void)
{
    return 0;
}

const EVP_CIPHER* CryptoNative_EvpAes256Ecb(void)
{
    return 0;
}

const EVP_CIPHER* CryptoNative_EvpAes256Gcm(void)
{
    return 0;
}

const EVP_CIPHER* CryptoNative_EvpAes256Wrap(void)
{
    return 0;
}

const EVP_CIPHER* CryptoNative_EvpAes256WrapPad(void)
{
    return 0;
}

const EVP_CIPHER* CryptoNative_EvpChaCha20Poly1305(void)
{
    return 0;
}

EVP_CIPHER_CTX* CryptoNative_EvpCipherCreate2(const EVP_CIPHER* type, uint8_t* key, int32_t keyLength, unsigned char* iv, int32_t enc)
{
    (void)type;
    (void)key;
    (void)keyLength;
    (void)iv;
    (void)enc;
    return 0;
}

EVP_CIPHER_CTX* CryptoNative_EvpCipherCreatePartial(const EVP_CIPHER* type)
{
    (void)type;
    return 0;
}

int32_t CryptoNative_EvpCipherCtxSetPadding(EVP_CIPHER_CTX* x, int32_t padding)
{
    (void)x;
    (void)padding;
    return 0;
}

void CryptoNative_EvpCipherDestroy(EVP_CIPHER_CTX* ctx)
{
    (void)ctx;
}

int32_t CryptoNative_EvpCipherFinalEx(EVP_CIPHER_CTX* ctx, uint8_t* outm, int32_t* outl)
{
    (void)ctx;
    (void)outm;
    (void)outl;
    return 0;
}

int32_t CryptoNative_EvpCipherGetAeadTag(EVP_CIPHER_CTX* ctx, uint8_t* tag, int32_t tagLength)
{
    (void)ctx;
    (void)tag;
    (void)tagLength;
    return 0;
}

int32_t CryptoNative_EvpCipherGetCcmTag(EVP_CIPHER_CTX* ctx, uint8_t* tag, int32_t tagLength)
{
    (void)ctx;
    (void)tag;
    (void)tagLength;
    return 0;
}

int32_t CryptoNative_EvpCipherGetGcmTag(EVP_CIPHER_CTX* ctx, uint8_t* tag, int32_t tagLength)
{
    (void)ctx;
    (void)tag;
    (void)tagLength;
    return 0;
}

int32_t CryptoNative_EvpCipherReset(EVP_CIPHER_CTX* ctx, uint8_t* pIv, int32_t cIv)
{
    (void)ctx;
    (void)pIv;
    (void)cIv;
    return 0;
}

int32_t CryptoNative_EvpCipherSetAeadTag(EVP_CIPHER_CTX* ctx, uint8_t* tag, int32_t tagLength)
{
    (void)ctx;
    (void)tag;
    (void)tagLength;
    return 0;
}

int32_t CryptoNative_EvpCipherSetCcmNonceLength(EVP_CIPHER_CTX* ctx, int32_t ivLength)
{
    (void)ctx;
    (void)ivLength;
    return 0;
}

int32_t CryptoNative_EvpCipherSetCcmTag(EVP_CIPHER_CTX* ctx, uint8_t* tag, int32_t tagLength)
{
    (void)ctx;
    (void)tag;
    (void)tagLength;
    return 0;
}

int32_t CryptoNative_EvpCipherSetGcmNonceLength(EVP_CIPHER_CTX* ctx, int32_t ivLength)
{
    (void)ctx;
    (void)ivLength;
    return 0;
}

int32_t CryptoNative_EvpCipherSetGcmTag(EVP_CIPHER_CTX* ctx, uint8_t* tag, int32_t tagLength)
{
    (void)ctx;
    (void)tag;
    (void)tagLength;
    return 0;
}

int32_t CryptoNative_EvpCipherSetKeyAndIV(EVP_CIPHER_CTX* ctx, uint8_t* key, unsigned char* iv, int32_t enc)
{
    (void)ctx;
    (void)key;
    (void)iv;
    (void)enc;
    return 0;
}

int32_t CryptoNative_EvpCipherUpdate(EVP_CIPHER_CTX* ctx, uint8_t* out, int32_t* outl, unsigned char* in, int32_t inl)
{
    (void)ctx;
    (void)out;
    (void)outl;
    (void)in;
    (void)inl;
    return 0;
}

const EVP_CIPHER* CryptoNative_EvpDes3Cbc(void)
{
    return 0;
}

const EVP_CIPHER* CryptoNative_EvpDes3Cfb64(void)
{
    return 0;
}

const EVP_CIPHER* CryptoNative_EvpDes3Cfb8(void)
{
    return 0;
}

const EVP_CIPHER* CryptoNative_EvpDes3Ecb(void)
{
    return 0;
}

const EVP_CIPHER* CryptoNative_EvpDesCbc(void)
{
    return 0;
}

const EVP_CIPHER* CryptoNative_EvpDesCfb8(void)
{
    return 0;
}

const EVP_CIPHER* CryptoNative_EvpDesEcb(void)
{
    return 0;
}

int32_t CryptoNative_EvpDigestCurrent(const EVP_MD_CTX* ctx, uint8_t* md, uint32_t* s)
{
    (void)ctx;
    (void)md;
    (void)s;
    return 0;
}

int32_t CryptoNative_EvpDigestCurrentXOF(const EVP_MD_CTX* ctx, uint8_t* md, uint32_t len)
{
    (void)ctx;
    (void)md;
    (void)len;
    return 0;
}

int32_t CryptoNative_EvpDigestFinalEx(EVP_MD_CTX* ctx, uint8_t* md, uint32_t* s)
{
    (void)ctx;
    (void)md;
    (void)s;
    return 0;
}

int32_t CryptoNative_EvpDigestFinalXOF(EVP_MD_CTX* ctx, uint8_t* md, uint32_t len)
{
    (void)ctx;
    (void)md;
    (void)len;
    return 0;
}

int32_t CryptoNative_EvpDigestOneShot(const EVP_MD* type, const void* source, int32_t sourceSize, uint8_t* md, uint32_t* mdSize)
{
    (void)type;
    (void)source;
    (void)sourceSize;
    (void)md;
    (void)mdSize;
    return 0;
}

int32_t CryptoNative_EvpDigestReset(EVP_MD_CTX* ctx, const EVP_MD* type)
{
    (void)ctx;
    (void)type;
    return 0;
}

int32_t CryptoNative_EvpDigestSqueeze(EVP_MD_CTX* ctx, uint8_t* md, uint32_t len, int32_t* haveFeature)
{
    (void)ctx;
    (void)md;
    (void)len;
    (void)haveFeature;
    return 0;
}

int32_t CryptoNative_EvpDigestUpdate(EVP_MD_CTX* ctx, const void* d, int32_t cnt)
{
    (void)ctx;
    (void)d;
    (void)cnt;
    return 0;
}

int32_t CryptoNative_EvpDigestXOFOneShot(const EVP_MD* type, const void* source, int32_t sourceSize, uint8_t* md, uint32_t len)
{
    (void)type;
    (void)source;
    (void)sourceSize;
    (void)md;
    (void)len;
    return 0;
}

EVP_KDF* CryptoNative_EvpKdfFetch(const char* algorithm, int32_t* haveFeature)
{
    (void)algorithm;
    (void)haveFeature;
    return 0;
}

void CryptoNative_EvpKdfFree(EVP_KDF* kdf)
{
    (void)kdf;
}

int32_t CryptoNative_EvpKemAvailable(const char* algorithm)
{
    (void)algorithm;
    return 0;
}

int32_t CryptoNative_EvpKemDecapsulate(EVP_PKEY* pKey, void* extraHandle, const uint8_t* ciphertext, int32_t ciphertextLength, uint8_t* sharedSecret, int32_t sharedSecretLength)
{
    (void)pKey;
    (void)extraHandle;
    (void)ciphertext;
    (void)ciphertextLength;
    (void)sharedSecret;
    (void)sharedSecretLength;
    return 0;
}

int32_t CryptoNative_EvpKemEncapsulate(EVP_PKEY* pKey, void* extraHandle, uint8_t* ciphertext, int32_t ciphertextLength, uint8_t* sharedSecret, int32_t sharedSecretLength)
{
    (void)pKey;
    (void)extraHandle;
    (void)ciphertext;
    (void)ciphertextLength;
    (void)sharedSecret;
    (void)sharedSecretLength;
    return 0;
}

int32_t CryptoNative_EvpKemExportDecapsulationKey(const EVP_PKEY* pKey, uint8_t* destination, int32_t destinationLength)
{
    (void)pKey;
    (void)destination;
    (void)destinationLength;
    return 0;
}

int32_t CryptoNative_EvpKemExportEncapsulationKey(const EVP_PKEY* pKey, uint8_t* destination, int32_t destinationLength)
{
    (void)pKey;
    (void)destination;
    (void)destinationLength;
    return 0;
}

int32_t CryptoNative_EvpKemExportPrivateSeed(const EVP_PKEY* pKey, uint8_t* destination, int32_t destinationLength)
{
    (void)pKey;
    (void)destination;
    (void)destinationLength;
    return 0;
}

EVP_PKEY* CryptoNative_EvpKemGeneratePkey(const char* kemName, uint8_t* seed, int32_t seedLength)
{
    (void)kemName;
    (void)seed;
    (void)seedLength;
    return 0;
}

int32_t CryptoNative_EvpKemGetPalId(const EVP_PKEY* pKey, int32_t* kemId, int32_t* hasSeed, int32_t* hasDecapsulationKey)
{
    (void)pKey;
    (void)kemId;
    (void)hasSeed;
    (void)hasDecapsulationKey;
    return 0;
}

EVP_MAC_CTX* CryptoNative_EvpMacCtxDup(const EVP_MAC_CTX* ctx)
{
    (void)ctx;
    return 0;
}

void CryptoNative_EvpMacCtxFree(EVP_MAC_CTX* ctx)
{
    (void)ctx;
}

EVP_MAC_CTX* CryptoNative_EvpMacCtxNew(EVP_MAC* mac)
{
    (void)mac;
    return 0;
}

int32_t CryptoNative_EvpMacCurrent(EVP_MAC_CTX* ctx, uint8_t* mac, int32_t macLength)
{
    (void)ctx;
    (void)mac;
    (void)macLength;
    return 0;
}

EVP_MAC* CryptoNative_EvpMacFetch(const char* algorithm, int32_t* haveFeature)
{
    (void)algorithm;
    (void)haveFeature;
    return 0;
}

int32_t CryptoNative_EvpMacFinal(EVP_MAC_CTX* ctx, uint8_t* mac, int32_t macLength)
{
    (void)ctx;
    (void)mac;
    (void)macLength;
    return 0;
}

void CryptoNative_EvpMacFree(EVP_MAC* mac)
{
    (void)mac;
}

int32_t CryptoNative_EvpMacInit(EVP_MAC_CTX* ctx, uint8_t* key, int32_t keyLength, uint8_t* customizationString, int32_t customizationStringLength, int32_t xof)
{
    (void)ctx;
    (void)key;
    (void)keyLength;
    (void)customizationString;
    (void)customizationStringLength;
    (void)xof;
    return 0;
}

int32_t CryptoNative_EvpMacOneShot(EVP_MAC* mac, uint8_t* key, int32_t keyLength, uint8_t* customizationString, int32_t customizationStringLength, const uint8_t* data, int32_t dataLength, uint8_t* destination, int32_t destinationLength, int32_t xof)
{
    (void)mac;
    (void)key;
    (void)keyLength;
    (void)customizationString;
    (void)customizationStringLength;
    (void)data;
    (void)dataLength;
    (void)destination;
    (void)destinationLength;
    (void)xof;
    return 0;
}

int32_t CryptoNative_EvpMacReset(EVP_MAC_CTX* ctx)
{
    (void)ctx;
    return 0;
}

int32_t CryptoNative_EvpMacUpdate(EVP_MAC_CTX* ctx, uint8_t* data, int32_t dataLength)
{
    (void)ctx;
    (void)data;
    (void)dataLength;
    return 0;
}

const EVP_MD* CryptoNative_EvpMd5(void)
{
    return 0;
}

EVP_MD_CTX* CryptoNative_EvpMdCtxCopyEx(const EVP_MD_CTX* ctx)
{
    (void)ctx;
    return 0;
}

EVP_MD_CTX* CryptoNative_EvpMdCtxCreate(const EVP_MD* type)
{
    (void)type;
    return 0;
}

void CryptoNative_EvpMdCtxDestroy(EVP_MD_CTX* ctx)
{
    (void)ctx;
}

int32_t CryptoNative_EvpMdSize(const EVP_MD* md)
{
    (void)md;
    return 0;
}

int32_t CryptoNative_EvpPKeyBits(EVP_PKEY* pkey)
{
    (void)pkey;
    return 0;
}

int32_t CryptoNative_EvpPKeyCreateByEcExplicitParameters(int32_t curveType, const uint8_t* qx, int32_t qxLength, const uint8_t* qy, int32_t qyLength, const uint8_t* d, int32_t dLength, const uint8_t* p, int32_t pLength, const uint8_t* a, int32_t aLength, const uint8_t* b, int32_t bLength, const uint8_t* gx, int32_t gxLength, const uint8_t* gy, int32_t gyLength, const uint8_t* order, int32_t orderLength, const uint8_t* cofactor, int32_t cofactorLength, const uint8_t* seed, int32_t seedLength, EVP_PKEY** pkey, int32_t* keySize)
{
    (void)curveType;
    (void)qx;
    (void)qxLength;
    (void)qy;
    (void)qyLength;
    (void)d;
    (void)dLength;
    (void)p;
    (void)pLength;
    (void)a;
    (void)aLength;
    (void)b;
    (void)bLength;
    (void)gx;
    (void)gxLength;
    (void)gy;
    (void)gyLength;
    (void)order;
    (void)orderLength;
    (void)cofactor;
    (void)cofactorLength;
    (void)seed;
    (void)seedLength;
    (void)pkey;
    (void)keySize;
    return 0;
}

int32_t CryptoNative_EvpPKeyCreateByEcParameters(EVP_PKEY** pkey, const char* oid, const uint8_t* qx, int32_t qxLength, const uint8_t* qy, int32_t qyLength, const uint8_t* d, int32_t dLength, int32_t* keySize)
{
    (void)pkey;
    (void)oid;
    (void)qx;
    (void)qxLength;
    (void)qy;
    (void)qyLength;
    (void)d;
    (void)dLength;
    (void)keySize;
    return 0;
}

EVP_PKEY* CryptoNative_EvpPKeyCreateRsa(RSA* currentKey)
{
    (void)currentKey;
    return 0;
}

int32_t CryptoNative_EvpPKeyDeriveSecretAgreement(EVP_PKEY* pkey, void* extraHandle, EVP_PKEY* peerKey, uint8_t* secret, uint32_t secretLength)
{
    (void)pkey;
    (void)extraHandle;
    (void)peerKey;
    (void)secret;
    (void)secretLength;
    return 0;
}

int32_t CryptoNative_EvpPKeyEcHasExplicitEncoding(EVP_PKEY* pkey)
{
    (void)pkey;
    return 0;
}

int32_t CryptoNative_EvpPKeyFamily(const EVP_PKEY* key)
{
    (void)key;
    return 0;
}

EVP_PKEY* CryptoNative_EvpPKeyFromData(const char* algorithmName, uint8_t* key, int32_t keyLength, int32_t privateKey)
{
    (void)algorithmName;
    (void)key;
    (void)keyLength;
    (void)privateKey;
    return 0;
}

int32_t CryptoNative_EvpPKeyGenerateByEcCurveOid(EVP_PKEY** pkey, const char* oid, int32_t* keySize)
{
    (void)pkey;
    (void)oid;
    (void)keySize;
    return 0;
}

int32_t CryptoNative_EvpPKeyGetEcCurveParameters(EVP_PKEY* pkey, int32_t includePrivate, int32_t* curveType, BIGNUM** qx, int32_t* cbQx, BIGNUM** qy, int32_t* cbQy, BIGNUM** d, int32_t* cbD, BIGNUM** p, int32_t* cbP, BIGNUM** a, int32_t* cbA, BIGNUM** b, int32_t* cbB, BIGNUM** gx, int32_t* cbGx, BIGNUM** gy, int32_t* cbGy, BIGNUM** order, int32_t* cbOrder, BIGNUM** cofactor, int32_t* cbCofactor, BIGNUM** seed, int32_t* cbSeed)
{
    (void)pkey;
    (void)includePrivate;
    (void)curveType;
    (void)qx;
    (void)cbQx;
    (void)qy;
    (void)cbQy;
    (void)d;
    (void)cbD;
    (void)p;
    (void)cbP;
    (void)a;
    (void)cbA;
    (void)b;
    (void)cbB;
    (void)gx;
    (void)cbGx;
    (void)gy;
    (void)cbGy;
    (void)order;
    (void)cbOrder;
    (void)cofactor;
    (void)cbCofactor;
    (void)seed;
    (void)cbSeed;
    return 0;
}

int32_t CryptoNative_EvpPKeyGetEcGroupNid(EVP_PKEY* pkey, int32_t* nidName)
{
    (void)pkey;
    (void)nidName;
    return 0;
}

int32_t CryptoNative_EvpPKeyGetEcKeyParameters(EVP_PKEY* pkey, int32_t includePrivate, BIGNUM** qx, int32_t* cbQx, BIGNUM** qy, int32_t* cbQy, BIGNUM** d, int32_t* cbD)
{
    (void)pkey;
    (void)includePrivate;
    (void)qx;
    (void)cbQx;
    (void)qy;
    (void)cbQy;
    (void)d;
    (void)cbD;
    return 0;
}

int32_t CryptoNative_EvpPKeyGetEcKeySize(EVP_PKEY* pkey)
{
    (void)pkey;
    return 0;
}

int32_t CryptoNative_EvpPKeySignPreEncoded(EVP_PKEY* pkey, void* extraHandle, uint8_t* msg, int32_t msgLen, uint8_t* destination, int32_t destinationLen)
{
    (void)pkey;
    (void)extraHandle;
    (void)msg;
    (void)msgLen;
    (void)destination;
    (void)destinationLen;
    return 0;
}

int32_t CryptoNative_EvpPKeySignPure(EVP_PKEY* pkey, void* extraHandle, uint8_t* msg, int32_t msgLen, uint8_t* context, int32_t contextLen, uint8_t* destination, int32_t destinationLen)
{
    (void)pkey;
    (void)extraHandle;
    (void)msg;
    (void)msgLen;
    (void)context;
    (void)contextLen;
    (void)destination;
    (void)destinationLen;
    return 0;
}

int32_t CryptoNative_EvpPKeyType(EVP_PKEY* key)
{
    (void)key;
    return 0;
}

int32_t CryptoNative_EvpPKeyVerifyPreEncoded(EVP_PKEY* pkey, void* extraHandle, uint8_t* msg, int32_t msgLen, uint8_t* sig, int32_t sigLen)
{
    (void)pkey;
    (void)extraHandle;
    (void)msg;
    (void)msgLen;
    (void)sig;
    (void)sigLen;
    return 0;
}

int32_t CryptoNative_EvpPKeyVerifyPure(EVP_PKEY* pkey, void* extraHandle, uint8_t* msg, int32_t msgLen, uint8_t* context, int32_t contextLen, uint8_t* sig, int32_t sigLen)
{
    (void)pkey;
    (void)extraHandle;
    (void)msg;
    (void)msgLen;
    (void)context;
    (void)contextLen;
    (void)sig;
    (void)sigLen;
    return 0;
}

EVP_PKEY* CryptoNative_EvpPkeyCreate(void)
{
    return 0;
}

void CryptoNative_EvpPkeyDestroy(EVP_PKEY* pkey)
{
    (void)pkey;
}

DSA* CryptoNative_EvpPkeyGetDsa(EVP_PKEY* pkey)
{
    (void)pkey;
    return 0;
}

int32_t CryptoNative_EvpPkeySetDsa(EVP_PKEY* pkey, DSA* dsa)
{
    (void)pkey;
    (void)dsa;
    return 0;
}

const EVP_CIPHER* CryptoNative_EvpRC2Cbc(void)
{
    return 0;
}

const EVP_CIPHER* CryptoNative_EvpRC2Ecb(void)
{
    return 0;
}

const EVP_MD* CryptoNative_EvpSha1(void)
{
    return 0;
}

const EVP_MD* CryptoNative_EvpSha256(void)
{
    return 0;
}

const EVP_MD* CryptoNative_EvpSha384(void)
{
    return 0;
}

const EVP_MD* CryptoNative_EvpSha3_256(void)
{
    return 0;
}

const EVP_MD* CryptoNative_EvpSha3_384(void)
{
    return 0;
}

const EVP_MD* CryptoNative_EvpSha3_512(void)
{
    return 0;
}

const EVP_MD* CryptoNative_EvpSha512(void)
{
    return 0;
}

const EVP_MD* CryptoNative_EvpShake128(void)
{
    return 0;
}

const EVP_MD* CryptoNative_EvpShake256(void)
{
    return 0;
}

void CryptoNative_ForEachTrackedAllocation(void (*callback)(void* ptr, uint64_t size, const char* file, int32_t line, void* ctx), void* ctx)
{
    (void)callback;
    (void)ctx;
}

int32_t CryptoNative_GetAsn1IntegerDerSize(ASN1_INTEGER* i)
{
    (void)i;
    return 0;
}

int32_t CryptoNative_GetAsn1StringBytes(ASN1_STRING* asn1, uint8_t* pBuf, int32_t cBuf)
{
    (void)asn1;
    (void)pBuf;
    (void)cBuf;
    return 0;
}

int32_t CryptoNative_GetBigNumBytes(const BIGNUM* a)
{
    (void)a;
    return 0;
}

int32_t CryptoNative_GetDefaultSignatureAlgorithms(uint16_t* buffer, int32_t* count)
{
    (void)buffer;
    (void)count;
    return 0;
}

int32_t CryptoNative_GetDsaParameters(const DSA* dsa, const BIGNUM** p, int32_t* pLength, const BIGNUM** q, int32_t* qLength, const BIGNUM** g, int32_t* gLength, const BIGNUM** y, int32_t* yLength, const BIGNUM** x, int32_t* xLength)
{
    (void)dsa;
    (void)p;
    (void)pLength;
    (void)q;
    (void)qLength;
    (void)g;
    (void)gLength;
    (void)y;
    (void)yLength;
    (void)x;
    (void)xLength;
    return 0;
}

int32_t CryptoNative_GetMaxMdSize(void)
{
    return 0;
}

int32_t CryptoNative_GetMemoryBioSize(BIO* bio)
{
    (void)bio;
    return 0;
}

void CryptoNative_GetMemoryUse(uint64_t* totalUsed, uint64_t* allocationCount)
{
    (void)totalUsed;
    (void)allocationCount;
}

const ASN1_OBJECT* CryptoNative_GetObjectDefinitionByName(const char* friendlyName)
{
    (void)friendlyName;
    return 0;
}

int32_t CryptoNative_GetOcspRequestDerSize(OCSP_REQUEST* req)
{
    (void)req;
    return 0;
}

const char* CryptoNative_GetOpenSslCipherSuiteName(SSL* ssl, int32_t cipherSuite, int32_t* isTls12OrLower)
{
    (void)ssl;
    (void)cipherSuite;
    (void)isTls12OrLower;
    return 0;
}

int32_t CryptoNative_GetPkcs7Certificates(PKCS7* p7, X509Stack** certs)
{
    (void)p7;
    (void)certs;
    return 0;
}

int32_t CryptoNative_GetPkcs7DerSize(PKCS7* p7)
{
    (void)p7;
    return 0;
}

int32_t CryptoNative_GetPkcs8PrivateKeySize(EVP_PKEY* pkey, int32_t* p8size)
{
    (void)pkey;
    (void)p8size;
    return 0;
}

int32_t CryptoNative_GetRandomBytes(uint8_t* buf, int32_t num)
{
    (void)buf;
    (void)num;
    return 0;
}

int32_t CryptoNative_GetSubjectPublicKeyInfoSize(EVP_PKEY* pkey)
{
    (void)pkey;
    return 0;
}

const ASN1_TIME* CryptoNative_GetX509CrlNextUpdate(X509_CRL* crl)
{
    (void)crl;
    return 0;
}

int32_t CryptoNative_GetX509DerSize(X509* x)
{
    (void)x;
    return 0;
}

EVP_PKEY* CryptoNative_GetX509EvpPublicKey(X509* x509)
{
    (void)x509;
    return 0;
}

BIO* CryptoNative_GetX509NameInfo(X509* x509, int32_t nameType, int32_t forIssuer)
{
    (void)x509;
    (void)nameType;
    (void)forIssuer;
    return 0;
}

int32_t CryptoNative_GetX509NameRawBytes(X509_NAME* x509Name, uint8_t* pBuf, int32_t cBuf)
{
    (void)x509Name;
    (void)pBuf;
    (void)cBuf;
    return 0;
}

X509_NAME* CryptoNative_GetX509NameStackField(X509NameStack* sk, int32_t loc)
{
    (void)sk;
    (void)loc;
    return 0;
}

int32_t CryptoNative_GetX509NameStackFieldCount(X509NameStack* sk)
{
    (void)sk;
    return 0;
}

const ASN1_TIME* CryptoNative_GetX509NotAfter(X509* x509)
{
    (void)x509;
    return 0;
}

const ASN1_TIME* CryptoNative_GetX509NotBefore(X509* x509)
{
    (void)x509;
    return 0;
}

ASN1_OBJECT* CryptoNative_GetX509PublicKeyAlgorithm(X509* x509)
{
    (void)x509;
    return 0;
}

const ASN1_BIT_STRING* CryptoNative_GetX509PublicKeyBytes(X509* x509)
{
    (void)x509;
    return 0;
}

int32_t CryptoNative_GetX509PublicKeyParameterBytes(X509* x509, uint8_t* pBuf, int32_t cBuf)
{
    (void)x509;
    (void)pBuf;
    (void)cBuf;
    return 0;
}

const char* CryptoNative_GetX509RootStoreFile(uint8_t* defaultPath)
{
    (void)defaultPath;
    return 0;
}

const char* CryptoNative_GetX509RootStorePath(uint8_t* defaultPath)
{
    (void)defaultPath;
    return 0;
}

ASN1_OBJECT* CryptoNative_GetX509SignatureAlgorithm(X509* x509)
{
    (void)x509;
    return 0;
}

X509* CryptoNative_GetX509StackField(X509_stack* stack, int loc)
{
    (void)stack;
    (void)loc;
    return 0;
}

int32_t CryptoNative_GetX509StackFieldCount(X509_stack* stack)
{
    (void)stack;
    return 0;
}

int32_t CryptoNative_GetX509SubjectPublicKeyInfoDerSize(X509* x)
{
    (void)x;
    return 0;
}

int32_t CryptoNative_GetX509Thumbprint(X509* x509, uint8_t* pBuf, int32_t cBuf)
{
    (void)x509;
    (void)pBuf;
    (void)cBuf;
    return 0;
}

int32_t CryptoNative_GetX509Version(X509* x509)
{
    (void)x509;
    return 0;
}

int32_t CryptoNative_HkdfDeriveKey(EVP_KDF* kdf, uint8_t* ikm, int32_t ikmLength, char* algorithm, uint8_t* salt, int32_t saltLength, uint8_t* info, int32_t infoLength, uint8_t* destination, int32_t destinationLength)
{
    (void)kdf;
    (void)ikm;
    (void)ikmLength;
    (void)algorithm;
    (void)salt;
    (void)saltLength;
    (void)info;
    (void)infoLength;
    (void)destination;
    (void)destinationLength;
    return 0;
}

int32_t CryptoNative_HkdfExpand(EVP_KDF* kdf, uint8_t* prk, int32_t prkLength, char* algorithm, uint8_t* info, int32_t infoLength, uint8_t* destination, int32_t destinationLength)
{
    (void)kdf;
    (void)prk;
    (void)prkLength;
    (void)algorithm;
    (void)info;
    (void)infoLength;
    (void)destination;
    (void)destinationLength;
    return 0;
}

int32_t CryptoNative_HkdfExtract(EVP_KDF* kdf, uint8_t* ikm, int32_t ikmLength, char* algorithm, uint8_t* salt, int32_t saltLength, uint8_t* destination, int32_t destinationLength)
{
    (void)kdf;
    (void)ikm;
    (void)ikmLength;
    (void)algorithm;
    (void)salt;
    (void)saltLength;
    (void)destination;
    (void)destinationLength;
    return 0;
}

DN_MAC_CTX* CryptoNative_HmacCopy(const DN_MAC_CTX* ctx)
{
    (void)ctx;
    return 0;
}

DN_MAC_CTX* CryptoNative_HmacCreate(uint8_t* key, int32_t keyLen, const EVP_MD* md)
{
    (void)key;
    (void)keyLen;
    (void)md;
    return 0;
}

int32_t CryptoNative_HmacCurrent(const DN_MAC_CTX* ctx, uint8_t* md, int32_t* len)
{
    (void)ctx;
    (void)md;
    (void)len;
    return 0;
}

void CryptoNative_HmacDestroy(DN_MAC_CTX* ctx)
{
    (void)ctx;
}

int32_t CryptoNative_HmacFinal(DN_MAC_CTX* ctx, uint8_t* md, int32_t* len)
{
    (void)ctx;
    (void)md;
    (void)len;
    return 0;
}

int32_t CryptoNative_HmacOneShot(const EVP_MD* type, const uint8_t* key, int32_t keySize, const uint8_t* source, int32_t sourceSize, uint8_t* md, int32_t* mdSize)
{
    (void)type;
    (void)key;
    (void)keySize;
    (void)source;
    (void)sourceSize;
    (void)md;
    (void)mdSize;
    return 0;
}

int32_t CryptoNative_HmacReset(DN_MAC_CTX* ctx)
{
    (void)ctx;
    return 0;
}

int32_t CryptoNative_HmacUpdate(DN_MAC_CTX* ctx, const uint8_t* data, int32_t len)
{
    (void)ctx;
    (void)data;
    (void)len;
    return 0;
}

int32_t CryptoNative_IsSignatureAlgorithmAvailable(const char* algorithm)
{
    (void)algorithm;
    return 0;
}

int32_t CryptoNative_IsSslRenegotiatePending(SSL* ssl)
{
    (void)ssl;
    return 0;
}

int32_t CryptoNative_IsSslStateOK(SSL* ssl)
{
    (void)ssl;
    return 0;
}

int32_t CryptoNative_KbkdfHmacOneShot(EVP_KDF* kdf, uint8_t* key, int32_t keyLength, char* algorithm, uint8_t* label, int32_t labelLength, uint8_t* context, int32_t contextLength, uint8_t* destination, int32_t destinationLength)
{
    (void)kdf;
    (void)key;
    (void)keyLength;
    (void)algorithm;
    (void)label;
    (void)labelLength;
    (void)context;
    (void)contextLength;
    (void)destination;
    (void)destinationLength;
    return 0;
}

EVP_PKEY* CryptoNative_LoadKeyFromProvider(const char** providerNames, int32_t providerNameCount, const char* keyUri, const char* propertyQuery, void** extraHandle, int32_t* haveProvider)
{
    (void)providerNames;
    (void)providerNameCount;
    (void)keyUri;
    (void)propertyQuery;
    (void)extraHandle;
    (void)haveProvider;
    return 0;
}

EVP_PKEY* CryptoNative_LoadPrivateKeyFromEngine(const char* engineName, const char* keyName, int32_t* haveEngine)
{
    (void)engineName;
    (void)keyName;
    (void)haveEngine;
    return 0;
}

EVP_PKEY* CryptoNative_LoadPublicKeyFromEngine(const char* engineName, const char* keyName, int32_t* haveEngine)
{
    (void)engineName;
    (void)keyName;
    (void)haveEngine;
    return 0;
}

int32_t CryptoNative_LookupFriendlyNameByOid(const char* oidValue, const char** friendlyName)
{
    (void)oidValue;
    (void)friendlyName;
    return 0;
}

int32_t CryptoNative_MLDsaExportPublicKey(const EVP_PKEY* pKey, uint8_t* destination, int32_t destinationLength)
{
    (void)pKey;
    (void)destination;
    (void)destinationLength;
    return 0;
}

int32_t CryptoNative_MLDsaExportSecretKey(const EVP_PKEY* pKey, uint8_t* destination, int32_t destinationLength)
{
    (void)pKey;
    (void)destination;
    (void)destinationLength;
    return 0;
}

int32_t CryptoNative_MLDsaExportSeed(const EVP_PKEY* pKey, uint8_t* destination, int32_t destinationLength)
{
    (void)pKey;
    (void)destination;
    (void)destinationLength;
    return 0;
}

EVP_PKEY* CryptoNative_MLDsaGenerateKey(const char* keyType, uint8_t* seed, int32_t seedLen)
{
    (void)keyType;
    (void)seed;
    (void)seedLen;
    return 0;
}

int32_t CryptoNative_MLDsaGetPalId(const EVP_PKEY* pKey, int32_t* mldsaId, int32_t* hasSeed, int32_t* hasSecretKey)
{
    (void)pKey;
    (void)mldsaId;
    (void)hasSeed;
    (void)hasSecretKey;
    return 0;
}

int32_t CryptoNative_MLDsaSignExternalMu(EVP_PKEY* pKey, void* extraHandle, uint8_t* mu, int32_t muLen, uint8_t* destination, int32_t destinationLen)
{
    (void)pKey;
    (void)extraHandle;
    (void)mu;
    (void)muLen;
    (void)destination;
    (void)destinationLen;
    return 0;
}

int32_t CryptoNative_MLDsaSignPreEncoded(EVP_PKEY* pkey, void* extraHandle, uint8_t* msg, int32_t msgLen, uint8_t* destination, int32_t destinationLen)
{
    (void)pkey;
    (void)extraHandle;
    (void)msg;
    (void)msgLen;
    (void)destination;
    (void)destinationLen;
    return 0;
}

int32_t CryptoNative_MLDsaSignPure(EVP_PKEY* pkey, void* extraHandle, uint8_t* msg, int32_t msgLen, uint8_t* context, int32_t contextLen, uint8_t* destination, int32_t destinationLen)
{
    (void)pkey;
    (void)extraHandle;
    (void)msg;
    (void)msgLen;
    (void)context;
    (void)contextLen;
    (void)destination;
    (void)destinationLen;
    return 0;
}

int32_t CryptoNative_MLDsaVerifyExternalMu(EVP_PKEY* pKey, void* extraHandle, uint8_t* mu, int32_t muLen, uint8_t* sig, int32_t sigLen)
{
    (void)pKey;
    (void)extraHandle;
    (void)mu;
    (void)muLen;
    (void)sig;
    (void)sigLen;
    return 0;
}

int32_t CryptoNative_MLDsaVerifyPreEncoded(EVP_PKEY* pkey, void* extraHandle, uint8_t* msg, int32_t msgLen, uint8_t* sig, int32_t sigLen)
{
    (void)pkey;
    (void)extraHandle;
    (void)msg;
    (void)msgLen;
    (void)sig;
    (void)sigLen;
    return 0;
}

int32_t CryptoNative_MLDsaVerifyPure(EVP_PKEY* pkey, void* extraHandle, uint8_t* msg, int32_t msgLen, uint8_t* context, int32_t contextLen, uint8_t* sig, int32_t sigLen)
{
    (void)pkey;
    (void)extraHandle;
    (void)msg;
    (void)msgLen;
    (void)context;
    (void)contextLen;
    (void)sig;
    (void)sigLen;
    return 0;
}

X509_stack* CryptoNative_NewX509Stack(void)
{
    return 0;
}

const ASN1_OBJECT* CryptoNative_ObjNid2Obj(int32_t nid)
{
    (void)nid;
    return 0;
}

int32_t CryptoNative_ObjObj2Txt(char* buf, int32_t buf_len, const ASN1_OBJECT* a)
{
    (void)buf;
    (void)buf_len;
    (void)a;
    return 0;
}

int32_t CryptoNative_ObjTxt2Nid(const char* sn)
{
    (void)sn;
    return 0;
}

const ASN1_OBJECT* CryptoNative_ObjTxt2Obj(const char* s)
{
    (void)s;
    return 0;
}

void CryptoNative_OcspRequestDestroy(OCSP_REQUEST* request)
{
    (void)request;
}

void CryptoNative_OcspResponseDestroy(OCSP_RESPONSE* response)
{
    (void)response;
}

int32_t CryptoNative_OpenSslAvailable(void)
{
    return 0;
}

int32_t CryptoNative_OpenSslGetProtocolSupport(int32_t protocol)
{
    (void)protocol;
    return 0;
}

int64_t CryptoNative_OpenSslVersionNumber(void)
{
    return 0;
}

int32_t CryptoNative_Pbkdf2(const char* password, int32_t passwordLength, const unsigned char* salt, int32_t saltLength, int32_t iterations, const EVP_MD* digest, unsigned char* destination, int32_t destinationLength)
{
    (void)password;
    (void)passwordLength;
    (void)salt;
    (void)saltLength;
    (void)iterations;
    (void)digest;
    (void)destination;
    (void)destinationLength;
    return 0;
}

PKCS7* CryptoNative_PemReadBioPkcs7(BIO* bp)
{
    (void)bp;
    return 0;
}

X509_CRL* CryptoNative_PemReadBioX509Crl(BIO* bio)
{
    (void)bio;
    return 0;
}

X509* CryptoNative_PemReadX509FromBio(BIO* bio)
{
    (void)bio;
    return 0;
}

X509* CryptoNative_PemReadX509FromBioAux(BIO* bio)
{
    (void)bio;
    return 0;
}

int32_t CryptoNative_PemWriteBioX509Crl(BIO* bio, X509_CRL* crl)
{
    (void)bio;
    (void)crl;
    return 0;
}

PKCS7* CryptoNative_Pkcs7CreateCertificateCollection(X509Stack* certs)
{
    (void)certs;
    return 0;
}

void CryptoNative_Pkcs7Destroy(PKCS7* p7)
{
    (void)p7;
}

int32_t CryptoNative_PushX509StackField(X509_stack* stack, X509* x509)
{
    (void)stack;
    (void)x509;
    return 0;
}

X509* CryptoNative_ReadX509AsDerFromBio(BIO* bio)
{
    (void)bio;
    return 0;
}

void CryptoNative_RecursiveFreeX509Stack(X509_stack* stack)
{
    (void)stack;
}

void CryptoNative_RegisterLegacyAlgorithms(void)
{
}

int32_t CryptoNative_RsaDecrypt(EVP_PKEY* pkey, void* extraHandle, const uint8_t* source, int32_t sourceLen, int32_t padding, const EVP_MD* digest, uint8_t* destination, int32_t destinationLen)
{
    (void)pkey;
    (void)extraHandle;
    (void)source;
    (void)sourceLen;
    (void)padding;
    (void)digest;
    (void)destination;
    (void)destinationLen;
    return 0;
}

int32_t CryptoNative_RsaEncrypt(EVP_PKEY* pkey, void* extraHandle, const uint8_t* source, int32_t sourceLen, int32_t padding, const EVP_MD* digest, uint8_t* destination, int32_t destinationLen)
{
    (void)pkey;
    (void)extraHandle;
    (void)source;
    (void)sourceLen;
    (void)padding;
    (void)digest;
    (void)destination;
    (void)destinationLen;
    return 0;
}

EVP_PKEY* CryptoNative_RsaGenerateKey(int32_t keySize)
{
    (void)keySize;
    return 0;
}

int32_t CryptoNative_RsaSignHash(EVP_PKEY* pkey, void* extraHandle, int32_t padding, const EVP_MD* digest, const uint8_t* hash, int32_t hashLen, uint8_t* destination, int32_t destinationLen)
{
    (void)pkey;
    (void)extraHandle;
    (void)padding;
    (void)digest;
    (void)hash;
    (void)hashLen;
    (void)destination;
    (void)destinationLen;
    return 0;
}

int32_t CryptoNative_RsaVerifyHash(EVP_PKEY* pkey, void* extraHandle, int32_t padding, const EVP_MD* digest, const uint8_t* hash, int32_t hashLen, const uint8_t* signature, int32_t signatureLen)
{
    (void)pkey;
    (void)extraHandle;
    (void)padding;
    (void)digest;
    (void)hash;
    (void)hashLen;
    (void)signature;
    (void)signatureLen;
    return 0;
}

int32_t CryptoNative_SlhDsaExportPublicKey(const EVP_PKEY* pKey, uint8_t* destination, int32_t destinationLength)
{
    (void)pKey;
    (void)destination;
    (void)destinationLength;
    return 0;
}

int32_t CryptoNative_SlhDsaExportSecretKey(const EVP_PKEY* pKey, uint8_t* destination, int32_t destinationLength)
{
    (void)pKey;
    (void)destination;
    (void)destinationLength;
    return 0;
}

EVP_PKEY* CryptoNative_SlhDsaGenerateKey(const char* keyType)
{
    (void)keyType;
    return 0;
}

int32_t CryptoNative_SlhDsaGetPalId(const EVP_PKEY* pKey, int32_t* slhDsaTypeId)
{
    (void)pKey;
    (void)slhDsaTypeId;
    return 0;
}

int32_t CryptoNative_SlhDsaSignPreEncoded(EVP_PKEY* pkey, void* extraHandle, uint8_t* msg, int32_t msgLen, uint8_t* destination, int32_t destinationLen)
{
    (void)pkey;
    (void)extraHandle;
    (void)msg;
    (void)msgLen;
    (void)destination;
    (void)destinationLen;
    return 0;
}

int32_t CryptoNative_SlhDsaSignPure(EVP_PKEY* pkey, void* extraHandle, uint8_t* msg, int32_t msgLen, uint8_t* context, int32_t contextLen, uint8_t* destination, int32_t destinationLen)
{
    (void)pkey;
    (void)extraHandle;
    (void)msg;
    (void)msgLen;
    (void)context;
    (void)contextLen;
    (void)destination;
    (void)destinationLen;
    return 0;
}

int32_t CryptoNative_SlhDsaVerifyPreEncoded(EVP_PKEY* pkey, void* extraHandle, uint8_t* msg, int32_t msgLen, uint8_t* sig, int32_t sigLen)
{
    (void)pkey;
    (void)extraHandle;
    (void)msg;
    (void)msgLen;
    (void)sig;
    (void)sigLen;
    return 0;
}

int32_t CryptoNative_SlhDsaVerifyPure(EVP_PKEY* pkey, void* extraHandle, uint8_t* msg, int32_t msgLen, uint8_t* context, int32_t contextLen, uint8_t* sig, int32_t sigLen)
{
    (void)pkey;
    (void)extraHandle;
    (void)msg;
    (void)msgLen;
    (void)context;
    (void)contextLen;
    (void)sig;
    (void)sigLen;
    return 0;
}

int32_t CryptoNative_SslAddClientCAs(SSL* ssl, X509** x509s, uint32_t count)
{
    (void)ssl;
    (void)x509s;
    (void)count;
    return 0;
}

int32_t CryptoNative_SslAddExtraChainCert(SSL* ssl, X509* x509)
{
    (void)ssl;
    (void)x509;
    return 0;
}

SSL* CryptoNative_SslCreate(SSL_CTX* ctx)
{
    (void)ctx;
    return 0;
}

int32_t CryptoNative_SslCtxAddExtraChainCert(SSL_CTX* ctx, X509* x509)
{
    (void)ctx;
    (void)x509;
    return 0;
}

int32_t CryptoNative_SslCtxCheckPrivateKey(SSL_CTX* ctx)
{
    (void)ctx;
    return 0;
}

SSL_CTX* CryptoNative_SslCtxCreate(const SSL_METHOD* method)
{
    (void)method;
    return 0;
}

void CryptoNative_SslCtxDestroy(SSL_CTX* ctx)
{
    (void)ctx;
}

void* CryptoNative_SslCtxGetData(SSL_CTX* ctx)
{
    (void)ctx;
    return 0;
}

int CryptoNative_SslCtxRemoveSession(SSL_CTX* ctx, SSL_SESSION* session)
{
    (void)ctx;
    (void)session;
    return 0;
}

void CryptoNative_SslCtxSetAlpnSelectCb(SSL_CTX* ctx, SslCtxSetAlpnCallback cb, void* arg)
{
    (void)ctx;
    (void)cb;
    (void)arg;
}

int CryptoNative_SslCtxSetCaching(SSL_CTX* ctx, int mode, int cacheSize, int contextIdLength, uint8_t* contextId, SslCtxNewSessionCallback newSessionCb, SslCtxRemoveSessionCallback removeSessionCb)
{
    (void)ctx;
    (void)mode;
    (void)cacheSize;
    (void)contextIdLength;
    (void)contextId;
    (void)newSessionCb;
    (void)removeSessionCb;
    return 0;
}

void CryptoNative_SslCtxSetCertVerifyCallback(SSL_CTX* ctx, SslCtxCertVerifyCallback callback)
{
    (void)ctx;
    (void)callback;
}

int32_t CryptoNative_SslCtxSetCiphers(SSL_CTX* ctx, const char* cipherList, const char* cipherSuites)
{
    (void)ctx;
    (void)cipherList;
    (void)cipherSuites;
    return 0;
}

int32_t CryptoNative_SslCtxSetData(SSL_CTX* ctx, void* ptr)
{
    (void)ctx;
    (void)ptr;
    return 0;
}

void CryptoNative_SslCtxSetDefaultOcspCallback(SSL_CTX* ctx)
{
    (void)ctx;
}

int32_t CryptoNative_SslCtxSetEncryptionPolicy(SSL_CTX* ctx, int32_t policy)
{
    (void)ctx;
    (void)policy;
    return 0;
}

void CryptoNative_SslCtxSetKeylogCallback(SSL_CTX* ctx, SslCtxSetKeylogCallback callback)
{
    (void)ctx;
    (void)callback;
}

void CryptoNative_SslCtxSetProtocolOptions(SSL_CTX* ctx, int32_t protocols)
{
    (void)ctx;
    (void)protocols;
}

void CryptoNative_SslCtxSetQuietShutdown(SSL_CTX* ctx)
{
    (void)ctx;
}

int32_t CryptoNative_SslCtxUseCertificate(SSL_CTX* ctx, X509* x)
{
    (void)ctx;
    (void)x;
    return 0;
}

int32_t CryptoNative_SslCtxUsePrivateKey(SSL_CTX* ctx, EVP_PKEY* pkey)
{
    (void)ctx;
    (void)pkey;
    return 0;
}

int32_t CryptoNative_SslDecrypt(SSL* ssl, uint8_t* inputPtr, int32_t inputLen, int32_t* consumed, uint8_t* outputPtr, int32_t outputCap, int32_t* leftoverOffset, int32_t* leftoverLength, int32_t* errorCode)
{
    (void)ssl;
    (void)inputPtr;
    (void)inputLen;
    (void)consumed;
    (void)outputPtr;
    (void)outputCap;
    (void)leftoverOffset;
    (void)leftoverLength;
    (void)errorCode;
    return 0;
}

void CryptoNative_SslDestroy(SSL* ssl)
{
    (void)ssl;
}

int32_t CryptoNative_SslDoHandshake(SSL* ssl, int32_t* errorCode)
{
    (void)ssl;
    (void)errorCode;
    return 0;
}

int32_t CryptoNative_SslEncrypt(SSL* ssl, const uint8_t* plaintextPtr, int32_t plaintextLen, uint8_t* outputPtr, int32_t outputCap, int32_t* outputWritten, int32_t* outputPending, int32_t* errorCode)
{
    (void)ssl;
    (void)plaintextPtr;
    (void)plaintextLen;
    (void)outputPtr;
    (void)outputCap;
    (void)outputWritten;
    (void)outputPending;
    (void)errorCode;
    return 0;
}

void CryptoNative_SslGet0AlpnSelected(SSL* ssl, const uint8_t** protocol, uint32_t* len)
{
    (void)ssl;
    (void)protocol;
    (void)len;
}

X509* CryptoNative_SslGetCertificate(SSL* ssl)
{
    (void)ssl;
    return 0;
}

X509NameStack* CryptoNative_SslGetClientCAList(SSL* ssl)
{
    (void)ssl;
    return 0;
}

int32_t CryptoNative_SslGetCurrentCipherId(SSL* ssl, int32_t* cipherId)
{
    (void)ssl;
    (void)cipherId;
    return 0;
}

void* CryptoNative_SslGetData(SSL* ssl)
{
    (void)ssl;
    return 0;
}

int32_t CryptoNative_SslGetError(SSL* ssl, int32_t ret)
{
    (void)ssl;
    (void)ret;
    return 0;
}

int32_t CryptoNative_SslGetFinished(SSL* ssl, void* buf, int32_t count)
{
    (void)ssl;
    (void)buf;
    (void)count;
    return 0;
}

X509Stack* CryptoNative_SslGetPeerCertChain(SSL* ssl)
{
    (void)ssl;
    return 0;
}

X509* CryptoNative_SslGetPeerCertificate(SSL* ssl)
{
    (void)ssl;
    return 0;
}

int32_t CryptoNative_SslGetPeerFinished(SSL* ssl, void* buf, int32_t count)
{
    (void)ssl;
    (void)buf;
    (void)count;
    return 0;
}

const char* CryptoNative_SslGetServerName(SSL* ssl)
{
    (void)ssl;
    return 0;
}

SSL_SESSION* CryptoNative_SslGetSession(SSL* ssl)
{
    (void)ssl;
    return 0;
}

SSL_CTX* CryptoNative_SslGetSslCtx(SSL* ssl)
{
    (void)ssl;
    return 0;
}

const char* CryptoNative_SslGetVersion(SSL* ssl)
{
    (void)ssl;
    return 0;
}

int32_t CryptoNative_SslHandshake(SSL* ssl, const uint8_t* inputPtr, int32_t inputLen, int32_t* consumed, uint8_t* outputPtr, int32_t outputCap, int32_t* outputWritten, int32_t* outputPending, int32_t* errorCode)
{
    (void)ssl;
    (void)inputPtr;
    (void)inputLen;
    (void)consumed;
    (void)outputPtr;
    (void)outputCap;
    (void)outputWritten;
    (void)outputPending;
    (void)errorCode;
    return 0;
}

int32_t CryptoNative_SslRead(SSL* ssl, void* buf, int32_t num, int32_t* error)
{
    (void)ssl;
    (void)buf;
    (void)num;
    (void)error;
    return 0;
}

int32_t CryptoNative_SslRenegotiate(SSL* ssl, int32_t* error)
{
    (void)ssl;
    (void)error;
    return 0;
}

void CryptoNative_SslSessionFree(SSL_SESSION* session)
{
    (void)session;
}

void* CryptoNative_SslSessionGetData(SSL_SESSION* session)
{
    (void)session;
    return 0;
}

const char* CryptoNative_SslSessionGetHostname(SSL_SESSION* session)
{
    (void)session;
    return 0;
}

int32_t CryptoNative_SslSessionReused(SSL* ssl)
{
    (void)ssl;
    return 0;
}

void CryptoNative_SslSessionSetData(SSL_SESSION* session, void* val)
{
    (void)session;
    (void)val;
}

int CryptoNative_SslSessionSetHostname(SSL_SESSION* session, const char* hostname)
{
    (void)session;
    (void)hostname;
    return 0;
}

void CryptoNative_SslSetAcceptMovingWriteBuffer(SSL* ssl)
{
    (void)ssl;
}

void CryptoNative_SslSetAcceptState(SSL* ssl)
{
    (void)ssl;
}

int32_t CryptoNative_SslSetAlpnProtos(SSL* ssl, const uint8_t* protos, uint32_t protos_len)
{
    (void)ssl;
    (void)protos;
    (void)protos_len;
    return 0;
}

void CryptoNative_SslSetBio(SSL* ssl, BIO* rbio, BIO* wbio)
{
    (void)ssl;
    (void)rbio;
    (void)wbio;
}

void CryptoNative_SslSetClientCertCallback(SSL* ssl, int set)
{
    (void)ssl;
    (void)set;
}

int32_t CryptoNative_SslSetClientSigalgs(SSL* ssl, uint8_t* str)
{
    (void)ssl;
    (void)str;
    return 0;
}

void CryptoNative_SslSetConnectState(SSL* ssl)
{
    (void)ssl;
}

int32_t CryptoNative_SslSetData(SSL* ssl, void* ptr)
{
    (void)ssl;
    (void)ptr;
    return 0;
}

int32_t CryptoNative_SslSetFd(SSL* ssl, intptr_t fd)
{
    (void)ssl;
    (void)fd;
    return 0;
}

void CryptoNative_SslSetPostHandshakeAuth(SSL* ssl, int32_t val)
{
    (void)ssl;
    (void)val;
}

void CryptoNative_SslSetQuietShutdown(SSL* ssl, int mode)
{
    (void)ssl;
    (void)mode;
}

int32_t CryptoNative_SslSetRetryVerify(SSL* ssl)
{
    (void)ssl;
    return 0;
}

int32_t CryptoNative_SslSetSession(SSL* ssl, SSL_SESSION* session)
{
    (void)ssl;
    (void)session;
    return 0;
}

int32_t CryptoNative_SslSetSigalgs(SSL* ssl, uint8_t* str)
{
    (void)ssl;
    (void)str;
    return 0;
}

int32_t CryptoNative_SslSetTlsExtHostName(SSL* ssl, uint8_t* name)
{
    (void)ssl;
    (void)name;
    return 0;
}

void CryptoNative_SslSetVerifyPeer(SSL* ssl, int32_t failIfNoPeerCert)
{
    (void)ssl;
    (void)failIfNoPeerCert;
}

int32_t CryptoNative_SslShutdown(SSL* ssl)
{
    (void)ssl;
    return 0;
}

void CryptoNative_SslStapleOcsp(SSL* ssl, uint8_t* buf, int32_t len)
{
    (void)ssl;
    (void)buf;
    (void)len;
}

void CryptoNative_SslUpdateOcspStaple(SSL* ssl, X509* cert)
{
    (void)ssl;
    (void)cert;
}

int32_t CryptoNative_SslUseCertificate(SSL* ssl, X509* x)
{
    (void)ssl;
    (void)x;
    return 0;
}

int32_t CryptoNative_SslUsePrivateKey(SSL* ssl, EVP_PKEY* pkey)
{
    (void)ssl;
    (void)pkey;
    return 0;
}

const SSL_METHOD* CryptoNative_SslV2_3Method(void)
{
    return 0;
}

int32_t CryptoNative_SslWrite(SSL* ssl, const void* buf, int32_t num, int32_t* error)
{
    (void)ssl;
    (void)buf;
    (void)num;
    (void)error;
    return 0;
}

int32_t CryptoNative_Tls13Supported(void)
{
    return 0;
}

int32_t CryptoNative_UpRefEvpPkey(EVP_PKEY* pkey)
{
    (void)pkey;
    return 0;
}

int32_t CryptoNative_X25519Available(void)
{
    return 0;
}

int32_t CryptoNative_X25519DeriveSecretAgreementWithBytes(EVP_PKEY* pkey, void* extraHandle, const uint8_t* peerKey, int32_t peerKeyLength, uint8_t* secret, uint32_t secretLength)
{
    (void)pkey;
    (void)extraHandle;
    (void)peerKey;
    (void)peerKeyLength;
    (void)secret;
    (void)secretLength;
    return 0;
}

int32_t CryptoNative_X25519ExportPrivateKey(const EVP_PKEY* key, uint8_t* destination, int32_t destinationLength)
{
    (void)key;
    (void)destination;
    (void)destinationLength;
    return 0;
}

int32_t CryptoNative_X25519ExportPublicKey(const EVP_PKEY* key, uint8_t* destination, int32_t destinationLength)
{
    (void)key;
    (void)destination;
    (void)destinationLength;
    return 0;
}

EVP_PKEY* CryptoNative_X25519GenerateKey(void)
{
    return 0;
}

EVP_PKEY* CryptoNative_X25519ImportPrivateKey(const uint8_t* source, int32_t sourceLength)
{
    (void)source;
    (void)sourceLength;
    return 0;
}

EVP_PKEY* CryptoNative_X25519ImportPublicKey(const uint8_t* source, int32_t sourceLength)
{
    (void)source;
    (void)sourceLength;
    return 0;
}

int32_t CryptoNative_X25519IsValidHandle(const EVP_PKEY* key, int32_t* hasPrivateKey)
{
    (void)key;
    (void)hasPrivateKey;
    return 0;
}

OCSP_REQUEST* CryptoNative_X509BuildOcspRequest(X509* subject, X509* issuer)
{
    (void)subject;
    (void)issuer;
    return 0;
}

OCSP_REQUEST* CryptoNative_X509ChainBuildOcspRequest(X509_STORE_CTX* storeCtx, int chainDepth)
{
    (void)storeCtx;
    (void)chainDepth;
    return 0;
}

int32_t CryptoNative_X509ChainGetCachedOcspStatus(X509_STORE_CTX* storeCtx, char* cachePath, int chainDepth)
{
    (void)storeCtx;
    (void)cachePath;
    (void)chainDepth;
    return 0;
}

int32_t CryptoNative_X509ChainHasStapledOcsp(X509_STORE_CTX* storeCtx)
{
    (void)storeCtx;
    return 0;
}

X509_STORE* CryptoNative_X509ChainNew(X509Stack* systemTrust, X509Stack* userTrust)
{
    (void)systemTrust;
    (void)userTrust;
    return 0;
}

int32_t CryptoNative_X509ChainVerifyOcsp(X509_STORE_CTX* storeCtx, OCSP_REQUEST* req, OCSP_RESPONSE* resp, char* cachePath, int chainDepth)
{
    (void)storeCtx;
    (void)req;
    (void)resp;
    (void)cachePath;
    (void)chainDepth;
    return 0;
}

int32_t CryptoNative_X509CheckPurpose(X509* x, int32_t id, int32_t ca)
{
    (void)x;
    (void)id;
    (void)ca;
    return 0;
}

void CryptoNative_X509CrlDestroy(X509_CRL* a)
{
    (void)a;
}

int32_t CryptoNative_X509DecodeOcspToExpiration(const uint8_t* buf, int32_t len, OCSP_REQUEST* req, X509* subject, X509** issuers, int issuersLen, int64_t* expiration)
{
    (void)buf;
    (void)len;
    (void)req;
    (void)subject;
    (void)issuers;
    (void)issuersLen;
    (void)expiration;
    return 0;
}

void CryptoNative_X509Destroy(X509* a)
{
    (void)a;
}

X509_EXTENSION* CryptoNative_X509ExtensionCreateByObj(ASN1_OBJECT* obj, int32_t isCritical, ASN1_OCTET_STRING* data)
{
    (void)obj;
    (void)isCritical;
    (void)data;
    return 0;
}

void CryptoNative_X509ExtensionDestroy(X509_EXTENSION* a)
{
    (void)a;
}

int32_t CryptoNative_X509ExtensionGetCritical(X509_EXTENSION* x)
{
    (void)x;
    return 0;
}

const ASN1_OCTET_STRING* CryptoNative_X509ExtensionGetData(X509_EXTENSION* x)
{
    (void)x;
    return 0;
}

const ASN1_OBJECT* CryptoNative_X509ExtensionGetOid(X509_EXTENSION* x)
{
    (void)x;
    return 0;
}

const ASN1_OCTET_STRING* CryptoNative_X509FindExtensionData(X509* x, int32_t nid)
{
    (void)x;
    (void)nid;
    return 0;
}

const X509_EXTENSION* CryptoNative_X509GetExt(X509* x, int32_t loc)
{
    (void)x;
    (void)loc;
    return 0;
}

int32_t CryptoNative_X509GetExtCount(X509* x)
{
    (void)x;
    return 0;
}

const X509_NAME* CryptoNative_X509GetIssuerName(X509* x509)
{
    (void)x509;
    return 0;
}

ASN1_INTEGER* CryptoNative_X509GetSerialNumber(X509* x509)
{
    (void)x509;
    return 0;
}

const X509_NAME* CryptoNative_X509GetSubjectName(X509* x509)
{
    (void)x509;
    return 0;
}

uint64_t CryptoNative_X509IssuerNameHash(X509* x)
{
    (void)x;
    return 0;
}

int32_t CryptoNative_X509StackAddDirectoryStore(X509Stack* stack, char* storePath)
{
    (void)stack;
    (void)storePath;
    return 0;
}

int32_t CryptoNative_X509StackAddMultiple(X509Stack* dest, X509Stack* src)
{
    (void)dest;
    (void)src;
    return 0;
}

int32_t CryptoNative_X509StoreAddCrl(X509_STORE* ctx, X509_CRL* x)
{
    (void)ctx;
    (void)x;
    return 0;
}

int32_t CryptoNative_X509StoreCtxCommitToChain(X509_STORE_CTX* storeCtx)
{
    (void)storeCtx;
    return 0;
}

X509_STORE_CTX* CryptoNative_X509StoreCtxCreate(void)
{
    return 0;
}

void CryptoNative_X509StoreCtxDestroy(X509_STORE_CTX* v)
{
    (void)v;
}

void* CryptoNative_X509StoreCtxGetAppData(X509_STORE_CTX* ctx)
{
    (void)ctx;
    return 0;
}

X509Stack* CryptoNative_X509StoreCtxGetChain(X509_STORE_CTX* ctx)
{
    (void)ctx;
    return 0;
}

X509* CryptoNative_X509StoreCtxGetCurrentCert(X509_STORE_CTX* ctx)
{
    (void)ctx;
    return 0;
}

int32_t CryptoNative_X509StoreCtxGetError(X509_STORE_CTX* ctx)
{
    (void)ctx;
    return 0;
}

int32_t CryptoNative_X509StoreCtxGetErrorDepth(X509_STORE_CTX* ctx)
{
    (void)ctx;
    return 0;
}

X509Stack* CryptoNative_X509StoreCtxGetSharedUntrusted(X509_STORE_CTX* ctx)
{
    (void)ctx;
    return 0;
}

SSL* CryptoNative_X509StoreCtxGetSslPtr(X509_STORE_CTX* storeCtx)
{
    (void)storeCtx;
    return 0;
}

int32_t CryptoNative_X509StoreCtxInit(X509_STORE_CTX* ctx, X509_STORE* store, X509* x509, X509Stack* extraStore)
{
    (void)ctx;
    (void)store;
    (void)x509;
    (void)extraStore;
    return 0;
}

int32_t CryptoNative_X509StoreCtxRebuildChain(X509_STORE_CTX* ctx)
{
    (void)ctx;
    return 0;
}

int32_t CryptoNative_X509StoreCtxReset(X509_STORE_CTX* ctx)
{
    (void)ctx;
    return 0;
}

int32_t CryptoNative_X509StoreCtxResetForSignatureError(X509_STORE_CTX* storeCtx, X509_STORE** newStore)
{
    (void)storeCtx;
    (void)newStore;
    return 0;
}

void CryptoNative_X509StoreCtxSetError(X509_STORE_CTX* storeCtx, int32_t error)
{
    (void)storeCtx;
    (void)error;
}

int32_t CryptoNative_X509StoreCtxSetVerifyCallback(X509_STORE_CTX* ctx, X509StoreVerifyCallback callback, void* appData)
{
    (void)ctx;
    (void)callback;
    (void)appData;
    return 0;
}

void CryptoNative_X509StoreDestroy(X509_STORE* v)
{
    (void)v;
}

int32_t CryptoNative_X509StoreSetRevocationFlag(X509_STORE* ctx, int32_t revocationFlag)
{
    (void)ctx;
    (void)revocationFlag;
    return 0;
}

int32_t CryptoNative_X509StoreSetVerifyTime(X509_STORE* ctx, int64_t unixTime)
{
    (void)ctx;
    (void)unixTime;
    return 0;
}

X509* CryptoNative_X509UpRef(X509* x509)
{
    (void)x509;
    return 0;
}

int32_t CryptoNative_X509V3ExtPrint(BIO* out, X509_EXTENSION* ext)
{
    (void)out;
    (void)ext;
    return 0;
}

int32_t CryptoNative_X509VerifyCert(X509_STORE_CTX* ctx)
{
    (void)ctx;
    return 0;
}

const char* CryptoNative_X509VerifyCertErrorString(int32_t n)
{
    (void)n;
    return 0;
}
