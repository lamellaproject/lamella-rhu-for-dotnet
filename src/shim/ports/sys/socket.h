/* The one socket call and its three modes the stock .NET serial layer's
 * sources reach for: the layer's close path calls
 * shutdown on the port's descriptor, which a terminal answers ENOTSOCK and
 * the layer ignores. The kit's C library has no sys/socket.h; nothing else
 * of the socket surface is declared here. */
#ifndef RHU_PORTS_SYS_SOCKET_H
#define RHU_PORTS_SYS_SOCKET_H

#define SHUT_RD 0
#define SHUT_WR 1
#define SHUT_RDWR 2

int shutdown(int fd, int how);

#endif /* RHU_PORTS_SYS_SOCKET_H */
