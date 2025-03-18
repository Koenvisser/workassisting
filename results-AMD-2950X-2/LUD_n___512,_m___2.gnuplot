set title "m = 2"
set terminal pdf size 2.2,2.0
set output "./results/LUD_n___512,_m___2.pdf"
set key off
set xrange [1:32]
set xtics (1, 4, 8, 12, 16, 20, 24, 28, 32)
set xlabel "Number of threads"
set yrange [0:9]
set ytics (0, 2, 4, 6, 8)
set ylabel "Speedup"
plot './results/LUD_n___512,_m___2.dat' using 1:2 title "Work stealing" pointsize 0.7 lw 1 pt 6 linecolor rgb "#5B2182" with linespoints, \
  './results/LUD_n___512,_m___2.dat' using 1:3 title "Multi-atomics 64 1 1" pointsize 0.7 lw 2 pt 1 linecolor rgb "#80141E" with linespoints, \
  './results/LUD_n___512,_m___2.dat' using 1:4 title "Multi-atomics 64 10 1" pointsize 0.7 lw 2 pt 1 linecolor rgb "#80C81E" with linespoints, \
  './results/LUD_n___512,_m___2.dat' using 1:5 title "Multi-atomics 64 1 4" pointsize 0.7 lw 2 pt 1 linecolor rgb "#801478" with linespoints, \
  './results/LUD_n___512,_m___2.dat' using 1:6 title "WorkAssisting 1" pointsize 0.4 lw 2 pt 7 linecolor rgb "#320A35" with linespoints, \
  './results/LUD_n___512,_m___2.dat' using 1:7 title "WorkAssisting 4" pointsize 0.4 lw 2 pt 7 linecolor rgb "#C80A35" with linespoints
