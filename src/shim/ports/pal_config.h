/* The configuration the stock .NET serial layer's sources are compiled
 * under on this system: the answers the upstream build
 * writes into its generated pal_config.h from its platform probes, given
 * here as fixed words for the one platform the shim presents. The three
 * capability words the layer branches on are set the Linux way (cfmakeraw,
 * cfsetspeed and the termios2 custom-rate path all served); the two
 * header probes for other systems are off; the error-string form is the
 * POSIX one the kit's C library declares; the string copy is strlcpy, which
 * the serial device unit defines, so no formatting routine is reached for;
 * every other name of the upstream template is off. The copied sources
 * under src/vendor/ports/ are compiled unchanged: this header and the
 * platform headers beside it are the whole of their configuration here. */
#ifndef RHU_PORTS_PAL_CONFIG_H
#define RHU_PORTS_PAL_CONFIG_H

#define HAVE_CFMAKERAW 1
#define HAVE_CFSETSPEED 1
#define HAVE_TERMIOS2 1
#define HAVE_IOSS_H 0
#define HAVE_SYS_FILIO_H 0
#define HAVE_GNU_STRERROR_R 0
#define HAVE_STRCPY_S 0
#define HAVE_STRLCPY 1
#define HAVE_BUILTIN_MUL_OVERFLOW 1

#define HAVE_LSEEK64 0
#define HAVE_MMAP64 0
#define HAVE_FTRUNCATE64 0
#define HAVE_POSIX_FADVISE64 0
#define HAVE_STATFS_VFS 0
#define HAVE_STATFS_MOUNT 0
#define HAVE_FLOCK64 0
#define HAVE_F_DUPFD_CLOEXEC 0
#define HAVE_F_DUPFD 0
#define HAVE_O_CLOEXEC 0
#define HAVE_GETIFADDRS 0
#define HAVE_UTSNAME_DOMAINNAME 0
#define HAVE_STAT64 0
#define HAVE_FORK 0
#define HAVE_POSIX_SPAWN_FILE_ACTIONS_ADDCHDIR_NP 0
#define HAVE_VFORK 0
#define HAVE_PR_SET_PDEATHSIG 0
#define HAVE_CLOSE_RANGE 0
#define HAVE_FDWALK 0
#define HAVE_CHMOD 0
#define HAVE_FCHMOD 0
#define HAVE_PIPE 0
#define HAVE_PIPE2 0
#define HAVE_STAT_BIRTHTIME 0
#define HAVE_STAT_TIMESPEC 0
#define HAVE_STAT_TIM 0
#define HAVE_STAT_NSEC 0
#define HAVE_STAT_FLAGS 0
#define HAVE_LCHFLAGS 0
#define HAVE_DIRENT_NAME_LEN 0
#define HAVE_MNTINFO 0
#define HAVE_STATFS_FSTYPENAME 0
#define HAVE_STATVFS_FSTYPENAME 0
#define HAVE_STATVFS_BASETYPE 0
#define HAVE_GETFSSTAT_INT 0
#define HAVE_GETFSSTAT_SIZE_T 0
#define HAVE_GETFSSTAT_LONG 0
#define HAVE_NON_LEGACY_STATFS 0
#define HAVE_SHM_OPEN_THAT_WORKS_WELL_ENOUGH_WITH_MMAP 0
#define HAVE_POSIX_ADVISE 0
#define HAVE_FALLOCATE 0
#define HAVE_PREADV 0
#define HAVE_PWRITEV 0
#define PRIORITY_REQUIRES_INT_WHO 0
#define KEVENT_REQUIRES_INT_PARAMS 0
#define HAVE_IOCTL 0
#define HAVE_IOCTL_WITH_INT_REQUEST 0
#define HAVE_TIOCGWINSZ 0
#define HAVE_SCHED_GETAFFINITY 0
#define HAVE_SCHED_SETAFFINITY 0
#define HAVE_SCHED_GETCPU 0
#define HAVE_PTHREAD_SETCANCELSTATE 0
#define HAVE_GNU_LIBNAMES_H 0
#define KEVENT_HAS_VOID_UDATA 0
#define HAVE_STATFS 0
#define HAVE_SYS_SOCKIO_H 0
#define HAVE_ETHTOOL_H 0
#define HAVE_SYS_POLL_H 0
#define HAVE_EPOLL 0
#define HAVE_GETHOSTNAME 0
#define HAVE_GETNAMEINFO 0
#define HAVE_SOCKADDR_UN_SUN_PATH 0
#define HAVE_ACCEPT4 0
#define HAVE_KQUEUE 0
#define HAVE_SENDFILE_4 0
#define HAVE_SENDFILE_6 0
#define HAVE_SENDFILE_7 0
#define HAVE_FCOPYFILE 0
#define HAVE_GETNAMEINFO_SIGNED_FLAGS 0
#define HAVE_GETPEEREID 0
#define HAVE_SUPPORT_FOR_DUAL_MODE_IPV4_PACKET_INFO 0
#define HAVE_IN_PKTINFO 0
#define HAVE_IP_MREQN 0
#define HAVE_IP_MULTICAST_IFINDEX 0
#define HAVE_NETINET_TCP_VAR_H 0
#define HAVE_NETINET_UDP_VAR_H 0
#define HAVE_NETINET_IP_VAR_H 0
#define HAVE_NETINET_ICMP_VAR_H 0
#define HAVE_IOS_NET_ROUTE_H 0
#define HAVE_IOS_NETINET_IP_VAR_H 0
#define HAVE_IOS_NETINET_ICMP_VAR_H 0
#define HAVE_IOS_NETINET_TCPFSM_H 0
#define HAVE_IOS_NETINET_UDP_VAR_H 0
#define HAVE_RT_MSGHDR 0
#define HAVE_RT_MSGHDR2 0
#define HAVE_IF_MSGHDR2 0
#define HAVE_SYS_SYSCTL_H 0
#define HAVE_SYS_IOCTL_H 0
#define HAVE_NETPACKET_PACKET_H 0
#define HAVE_NET_IF_ARP_H 0
#define HAVE_SYS_MNTENT_H 0
#define HAVE_MNTENT_H 0
#define HAVE_NET_IFMEDIA_H 0
#define HAVE_IOS_NET_IFMEDIA_H 0
#define HAVE_LINUX_RTNETLINK_H 0
#define HAVE_LINUX_CAN_H 0
#define HAVE_LINUX_ERRQUEUE_H 0
#define HAVE_GETDOMAINNAME_SIZET 0
#define HAVE_INOTIFY 0
#define HAVE_CLOCK_REALTIME 0
#define HAVE_CLOCK_GETTIME_NSEC_NP 0
#define HAVE_PTHREAD_CONDATTR_SETCLOCK 0
#define HAVE_PTHREAD_MUTEX_CLOCKLOCK 0
#define HAVE_TCP_H_TCPSTATE_ENUM 0
#define HAVE_TCP_FSM_H 0
#define HAVE_GSSFW_HEADERS 0
#define HAVE_GSS_SPNEGO_MECHANISM 0
#define HAVE_HEIMDAL_HEADERS 0
#define HAVE_NSGETENVIRON 0
#define HAVE_GETAUXVAL 0
#define HAVE_CRT_EXTERNS_H 0
#define HAVE_GETDOMAINNAME 0
#define HAVE_UTIMENSAT 0
#define HAVE_LUTIMES 0
#define HAVE_FUTIMES 0
#define HAVE_FUTIMENS 0
#define HAVE_MKSTEMPS 0
#define HAVE_MKSTEMP 0
#define IPV6MR_INTERFACE_UNSIGNED 0
#define INOTIFY_RM_WATCH_WD_UNSIGNED 0
#define HAVE_IN_EXCL_UNLINK 0
#define HAVE_TCP_H_TCP_KEEPALIVE 0
#define HAVE_DISCONNECTX 0
#define HAVE_CONNECTX 0
#define HAVE_GETGROUPLIST 0
#define HAVE_SYSLOG_H 0
#define HAVE_TERMIOS_H 1
#define HAVE_DLFCN_H 0
#define HAVE_PTHREAD_H 0
#define HAVE_SYS_STATFS_H 0
#define HAVE_SYS_STATVFS_H 0
#define HAVE_NET_IF_H 0
#define HAVE_SYS_PROCINFO_H 0
#define HAVE_OS_H 0
#define HAVE_ALIGNED_ALLOC 0
#define HAVE_MALLOC_SIZE 0
#define HAVE_MALLOC_USABLE_SIZE 0
#define HAVE_MALLOC_USABLE_SIZE_NP 0
#define HAVE_POSIX_MEMALIGN 0
#define HAVE_MAKEDEV_FILEH 0
#define HAVE_MAKEDEV_SYSMACROSH 0
#define HAVE_GETGRGID_R 0

#endif /* RHU_PORTS_PAL_CONFIG_H */
