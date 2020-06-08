# swyt: Stop wasting your time

Swyt is a daemon that will kill processes based on time period rules.

## Configuration 

Unix-like filepath: *.config/swyt/config.jbb*

Windows filepath: *AppData/Roaming/swyt/config.jbb*

This files contains the settings of the daemon
```
check_interval=60 # The number of seconds the daemon will sleep before checking the process list
```
\
Unix-like filepath: *.config/swyt/rules.jbb*

Windows filepath: *AppData/Roaming/swyt/rules.jbb*

This file contains the rules, the rules describe the time where the process is allowed to run and wont be killed.

Rules are written using the following format:

``process_name=PERIOD1|PERIOD2|...``

A period is described as such:

``begin_time1~end_time1,begin_time2~end_time2:day_of_week1,day_of_week2,...``

Example: ``17:00~20:00;MO,TU,WE``

You can also specify the entire day using ``*``

Example: ``*;SA,SU``

\
Here is a full example of what the rules.jbb file might look like
```
my_chat_app=12:00~14:00,18:00~22:00;MO,TU,WE,TH,FR|*;SA,SU
work_related_app=8:00~18:00;MO,TU,WE,TH,FR|8:00~12:00;SA,SU
```


## Protip
Use swyt as a systemd service !