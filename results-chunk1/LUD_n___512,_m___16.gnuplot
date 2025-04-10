set title "m = 16"
set terminal pdf size 2.2,2.0
set output "./results/LUD_n___512,_m___16.pdf"
set key off
set xrange [1:32]
set xtics (1, 4, 8, 12, 16, 20, 24, 28, 32)
set xlabel "Number of threads"
set yrange [0:9]
set ytics (0, 2, 4, 6, 8)
set ylabel "Speedup"
plot './results/LUD_n___512,_m___16.dat' using 1:2 title "Work stealing" pointsize 0.7 lw 1 pt 6 linecolor rgb "#5B2182" with linespoints, \
  './results/LUD_n___512,_m___16.dat' using 1:3 title "Multi-atomics 64" pointsize 0.7 lw 2 pt 1 linecolor rgb "#64D000" with linespoints, \
  './results/LUD_n___512,_m___16.dat' using 1:4 title "WorkAssisting" pointsize 0.4 lw 2 pt 7 linecolor rgb "#F40A35" with linespoints
