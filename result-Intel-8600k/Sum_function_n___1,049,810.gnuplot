set title "Sum function (n = 1,049,810)"
set terminal pdf size 3.2,2.9
set output "./results/Sum_function_n___1,049,810.pdf"
set key off
set xrange [1:6]
set xtics (1, 4)
set xlabel "Number of threads"
set yrange [0:16]
set ylabel "Speedup"
plot './results/Sum_function_n___1,049,810.dat' using 1:2 title "Rayon" pointsize 0.7 lw 1 pt 1 linecolor rgb "#6E3B23" with linespoints, \
  './results/Sum_function_n___1,049,810.dat' using 1:3 title "Static" pointsize 0.7 lw 1 pt 2 linecolor rgb "#5287C6" with linespoints, \
  './results/Sum_function_n___1,049,810.dat' using 1:4 title "Static (pinned)" pointsize 0.7 lw 1 pt 3 linecolor rgb "#24A793" with linespoints, \
  './results/Sum_function_n___1,049,810.dat' using 1:5 title "Work stealing" pointsize 0.7 lw 1 pt 6 linecolor rgb "#5B2182" with linespoints, \
  './results/Sum_function_n___1,049,810.dat' using 1:6 title "OpenMP (static)" pointsize 0.7 lw 1 pt 5 linecolor rgb "#FFCD00" with linespoints, \
  './results/Sum_function_n___1,049,810.dat' using 1:7 title "OpenMP (dynamic)" pointsize 0.7 lw 1 pt 4 linecolor rgb "#001240" with linespoints, \
  './results/Sum_function_n___1,049,810.dat' using 1:8 title "OpenMP (taskloop)" pointsize 0.7 lw 1 pt 12 linecolor rgb "#F3965E" with linespoints, \
  './results/Sum_function_n___1,049,810.dat' using 1:9 title "Work assisting (our)" pointsize 0.4 lw 2 pt 7 linecolor rgb "#C00A35" with linespoints
