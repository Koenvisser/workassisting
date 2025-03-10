set title "Primes (2 .. 1,048,578)"
set terminal pdf size 3.2,2.9
set output "./results/Primes_2_.._1,048,578.pdf"
set key on
set key top left Left reverse
set key samplen 2.5
set xrange [1:32]
set xtics (1, 4, 8, 12, 16, 20, 24, 28, 32)
set xlabel "Number of threads"
set yrange [0:16]
set ylabel "Speedup"
plot './results/Primes_2_.._1,048,578.dat' using 1:2 title "Rayon" pointsize 0.7 lw 1 pt 1 linecolor rgb "#6E3B23" with linespoints, \
  './results/Primes_2_.._1,048,578.dat' using 1:3 title "Static" pointsize 0.7 lw 1 pt 2 linecolor rgb "#5287C6" with linespoints, \
  './results/Primes_2_.._1,048,578.dat' using 1:4 title "Static (pinned)" pointsize 0.7 lw 1 pt 3 linecolor rgb "#24A793" with linespoints, \
  './results/Primes_2_.._1,048,578.dat' using 1:5 title "Work stealing" pointsize 0.7 lw 1 pt 6 linecolor rgb "#5B2182" with linespoints, \
  './results/Primes_2_.._1,048,578.dat' using 1:6 title "Multi-atomics 64 1 1" pointsize 0.7 lw 2 pt 1 linecolor rgb "#80141E" with linespoints, \
  './results/Primes_2_.._1,048,578.dat' using 1:7 title "Multi-atomics 32 1 1" pointsize 0.7 lw 2 pt 1 linecolor rgb "#40141E" with linespoints, \
  './results/Primes_2_.._1,048,578.dat' using 1:8 title "Multi-atomics 64 10 1" pointsize 0.7 lw 2 pt 1 linecolor rgb "#80C81E" with linespoints, \
  './results/Primes_2_.._1,048,578.dat' using 1:9 title "Multi-atomics 64 1 4" pointsize 0.7 lw 2 pt 1 linecolor rgb "#801478" with linespoints, \
  './results/Primes_2_.._1,048,578.dat' using 1:10 title "Multi-atomics 64 10 4" pointsize 0.7 lw 2 pt 1 linecolor rgb "#80C878" with linespoints, \
  './results/Primes_2_.._1,048,578.dat' using 1:11 title "Work assisting" pointsize 0.4 lw 2 pt 7 linecolor rgb "#C00A35" with linespoints
