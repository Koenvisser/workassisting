set title "m = 32"
set terminal pdf size 2.2,2.0
set output "./results/LUD_n___512,_m___32.pdf"
set key off
set xrange [1:32]
set xtics (1, 4, 8, 12, 16, 20, 24, 28, 32)
set xlabel "Number of threads"
set yrange [0:9]
set ytics (0, 2, 4, 6, 8)
set ylabel "Speedup"
plot './results/LUD_n___512,_m___32.dat' using 1:2 title "Work stealing" pointsize 0.7 lw 1 pt 6 linecolor rgb "#5B2182" with linespoints, \
  './results/LUD_n___512,_m___32.dat' using 1:3 title "Multi-atomics 64 1 1" pointsize 0.7 lw 2 pt 1 linecolor rgb "#80141E" with linespoints, \
  './results/LUD_n___512,_m___32.dat' using 1:4 title "Multi-atomics 32 1 1" pointsize 0.7 lw 2 pt 1 linecolor rgb "#40141E" with linespoints, \
  './results/LUD_n___512,_m___32.dat' using 1:5 title "Multi-atomics 64 10 1" pointsize 0.7 lw 2 pt 1 linecolor rgb "#80C81E" with linespoints, \
  './results/LUD_n___512,_m___32.dat' using 1:6 title "Multi-atomics 64 1 4" pointsize 0.7 lw 2 pt 1 linecolor rgb "#801478" with linespoints, \
  './results/LUD_n___512,_m___32.dat' using 1:7 title "Multi-atomics 64 10 4" pointsize 0.7 lw 2 pt 1 linecolor rgb "#80C878" with linespoints, \
  './results/LUD_n___512,_m___32.dat' using 1:8 title "Work assisting" pointsize 0.4 lw 2 pt 7 linecolor rgb "#C00A35" with linespoints
