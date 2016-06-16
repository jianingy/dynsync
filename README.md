Intro
------

a tool for synchronize files to multiple locations automatically.

Usage
------

    Usage: dynsync [options] <dest>...

    Options:
        --debug                       enable debug
        --rsync=<path>                path to rsync binary
                                      [default: /usr/bin/rsync]
        --root=<directory>            root directory to watch
        -x, --ignore-file=<file>      a file of regular expressions of filenames to ignore
        -P, --rsync-params=<params>   rsync parameters separated by whitespaces
        -h, --help                    show this message
        -v, --version                 show version

    Bug reports to jianingy.yang@gmail.com
