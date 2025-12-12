# 1brc

1brc for fun

- run `./dev check` for all checks
- run `./dev time` for measure the running time
- run `./dev perf` for performance stat

```
hyperfine --warmup 3 ./target/release/y1brc
Benchmark 1: ./target/release/y1brc
  Time (mean ± σ):      1.189 s ±  0.022 s    [User: 22.846 s, System: 0.434 s]
  Range (min … max):    1.154 s …  1.223 s    10 runs
 
```

```
neofetch
                   -`                    xxx@archlinux 
                  .o+`                   ------------- 
                 `ooo/                   OS: Arch Linux x86_64 
                `+oooo:                  Host: OptiPlex Micro Plus 7010 
               `+oooooo:                 Kernel: 6.8.9-arch1-2 
               -+oooooo+:                Uptime: 123 days, 19 hours, 4 mins 
             `/:-:++oooo+:               Packages: 858 (pacman) 
            `/++++/+++++++:              Shell: zsh 5.9 
           `/++++++++++++++:             Terminal: /dev/pts/4 
          `/+++ooooooooooooo/`           CPU: 13th Gen Intel i9-13900T (32) @ 5.100GHz 
         ./ooosssso++osssssso+`          GPU: Intel Raptor Lake-S GT1 [UHD Graphics 770] 
        .oossssso-````/ossssss+`         Memory: 4792MiB / 63992MiB 
       -osssssso.      :ssssssso.
      :osssssss/        osssso+++.                               
     /ossssssss/        +ssssooo/-                               
   `/ossssso+/:-        -:/+osssso+-
  `+sso+:-`                 `.-/+oso:
 `++:.                           `-/+/
 .`                                 `/
```
