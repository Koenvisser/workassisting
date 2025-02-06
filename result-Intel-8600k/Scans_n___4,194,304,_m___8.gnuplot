set title "m = 8"
set terminal pdf size 2.2,2.0
set output "./results/Scans_n___4,194,304,_m___8.pdf"
set key off
set xrange [1:6]
set xtics (1, 4)
set xlabel "Number of threads"
set yrange [0:5]
set ytics (0, 1, 2, 3, 4)
set ylabel "Speedup"
plot './results/Scans_n___4,194,304,_m___8.dat' using 1:2 title "Outer parallelism" pointsize 0.7 lw 1 pt 1 linecolor rgb "#24A793" with linespoints, \
  './results/Scans_n___4,194,304,_m___8.dat' using 1:3 title "Inner parallelism" pointsize 0.7 lw 1 pt 2 linecolor rgb "#5287C6" with linespoints, \
  './results/Scans_n___4,194,304,_m___8.dat' using 1:4 title "OpenMP" pointsize 0.7 lw 1 pt 4 linecolor rgb "#001240" with linespoints, \
  './results/Scans_n___4,194,304,_m___8.dat' using 1:5 title "Work assisting (our)" pointsize 0.4 lw 2 pt 7 linecolor rgb "#C00A35" with linespoints
